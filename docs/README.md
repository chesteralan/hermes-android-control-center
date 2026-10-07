# Documentation Index

Hermes Android Control Center (HACC) is a desktop app (Tauri 2 + Rust + React) for managing a Hermes Agent running in Termux on an Android phone over Wireless ADB. macOS ships first (v1.0); Windows and Linux follow in v1.1 (M12).

Open the [single-page documentation hub](index.html) for a searchable, grouped view of the project docs.

| Document | Purpose |
|---|---|
| [INITIAL.md](INITIAL.md) | Original product brief (source of truth for requirements) |
| [BRAINSTORM.md](BRAINSTORM.md) | Problem exploration, critical risks, options considered |
| [ARCHITECTURE.md](ARCHITECTURE.md) | System design, modules, traits, IPC contract, data types |
| [DECISIONS.md](DECISIONS.md) | Architecture Decision Records (ADRs) |
| [MILESTONES.md](MILESTONES.md) | Roadmap from empty repo to production release |
| [TODO.md](TODO.md) | Consolidated open tickets and milestone exit checks |
| [tasks/](tasks/) | Per-milestone task breakdowns with acceptance criteria |
| [TESTING.md](TESTING.md) | Test strategy, mock ADB fixtures, CI gates |
| [SECURITY.md](SECURITY.md) | Threat model and security controls |
| [RELEASE.md](RELEASE.md) | Build, signing, notarization, versioning, release checklist |
| [guides/ANDROID_SETUP.md](guides/ANDROID_SETUP.md) | ADB, Developer Options, pairing, Termux, Hermes setup |
| [guides/DESKTOP_PLATFORMS.md](guides/DESKTOP_PLATFORMS.md) | Windows/Linux setup, bundles, native/encrypted secret storage, and certification |
| [guides/TROUBLESHOOTING.md](guides/TROUBLESHOOTING.md) | Common ADB / Termux / Hermes failures |

## Reading order

1. BRAINSTORM → understand the hard problems (especially the **Termux access boundary**).
2. ARCHITECTURE + DECISIONS → understand the shape of the code.
3. MILESTONES → pick the current milestone.
4. `tasks/Mx-*.md` → execute tasks in order; each task has a Definition of Done.

## Status legend used in task files

- `[ ]` not started · `[~]` in progress · `[x]` done · `[!]` blocked
