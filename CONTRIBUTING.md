# Contributing

Thanks for helping improve Hermes Control Center. Check [docs/TODO.md](docs/TODO.md) and the relevant milestone task file before starting work. For larger changes, open an issue first to agree on scope.

## Development setup

Requirements: Node.js LTS, Rust stable, and the platform's Tauri prerequisites. A physical Android device is not required for automated tests. Install dependencies and start the desktop app with:

```sh
npm ci
npm run tauri dev
```

## Checks

Run the checks used by CI before opening a pull request:

```sh
npm run lint
npm run typecheck
npm test -- --run
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm run test:rust
npm run gen:types && git diff --exit-code src/types/generated
```

The CI workflow also builds the unsigned app. See [docs/TESTING.md](docs/TESTING.md) for test boundaries and fixture rules.

## Pull requests

- Keep changes scoped to the related task and follow [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).
- Add or update tests; automated tests must not require a physical phone.
- Never commit credentials, private keys, pairing codes, or unredacted device identifiers. Redact serials and IP addresses in fixtures.
- Regenerate Rust-derived TypeScript bindings with `npm run gen:types`; do not edit generated files by hand.
- Update documentation for behavior or configuration changes. Include screenshots for user-visible UI changes when practical.
- Complete the repository's [pull request checklist](.github/pull_request_template.md).
