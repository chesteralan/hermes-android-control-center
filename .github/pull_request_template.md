## What

## Why

## Definition of Done (docs/MILESTONES.md)
- [ ] No `adb` spawn outside `AdbClient`/`ProcessRunner`; no single-device assumptions; OS-specific code only in `platform`
- [ ] Unit tests added/updated (no physical device required)
- [ ] `npm run lint`, `npm run typecheck`, `npm test`, `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test` green
- [ ] Errors map to an `AppError` with a human message and details
- [ ] No `any`; generated types not hand-edited
- [ ] Docs updated if behavior or config changed
