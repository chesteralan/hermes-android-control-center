# Hermes Control Center

Desktop app (Tauri 2 + Rust + React) for managing a [Hermes Agent](https://github.com/NousResearch/hermes-agent) running in Termux on an Android phone over Wireless ADB. No screen mirroring needed.

**Status:** M0 foundations are in progress (ADB fixture matrix and fresh-phone provisioning checks remain); M1–M4 implementation milestones are complete. M5 and later work is tracked in [docs/MILESTONES.md](docs/MILESTONES.md).

## What works today
- ADB auto-detection (or custom path in Settings)
- Live device list (`adb track-devices`), multiple phones
- Pair with **QR code** or pairing code, connect by IP:port, mDNS discovery
- Device info: model, Android/SDK, IP, battery, storage, memory, CPU, Termux package
- Automatic reconnect with backoff (1s → 30s), manual retry
- Persistent settings
- Termux SSH bridge over `adb forward` with per-device key and pinned host key
- Hermes status, installation/version detection, Start/Stop/Restart, Doctor and Update controls (M5; real-phone action verification pending)

## Prerequisites
- Node.js LTS, Rust stable, Xcode Command Line Tools (macOS)
- `adb` (e.g. `brew install --cask android-platform-tools`)
- An Android 11+ phone with Wireless debugging — see [docs/guides/ANDROID_SETUP.md](docs/guides/ANDROID_SETUP.md)

## Run
```sh
npm install
npm run tauri dev      # development
npm run tauri build    # production bundle
```

## Test
```sh
npm test -- --run      # frontend (Vitest)
npm run test:rust      # backend (cargo test, uses recorded adb fixtures — no phone needed)
npm run lint && npm run typecheck
```

## Docs
Start at [docs/README.md](docs/README.md). Troubleshooting: [docs/guides/TROUBLESHOOTING.md](docs/guides/TROUBLESHOOTING.md).
