# M10-T1 Error Audit Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close M10-T1's two open exit items — (1) re-run the non-test panic/swallow grep audit on the post-merge tree (the 2026-10-13 sweep predates ~2.3k lines of new code) and (2) reproduce every brief §16 failure case through its real generation path, verified to show a human message + non-empty expandable Details.

**Architecture:** All local work is hermetic: `FakeRunner` (`src-tauri/src/process.rs`) injects subprocess output into `AdbClient`/transport code paths, and the resulting `AppError::to_payload()` is asserted against brief §16's contract. The frontend's `ErrorPanel` gets a rendering verification that the Details expander reveals technical content. Phone-gated cases are evaluated for local reachability, then documented as live-matrix-blocked if not.

**Tech Stack:** Rust/Tauri 2 (`cargo test`, strict Clippy), React + Vitest, ripgrep for the audit sweep.

**Spec:** `docs/tasks/M10-production.md` §M10-T1 (lines 7–31) implementing brief §16 (`docs/INITIAL.md:518-547`).

## Global Constraints

- Brief §16 contract (`docs/INITIAL.md:518-547`): every listed failure shows a human-readable message, technical details behind an expandable "Details" section, and errors are never silently swallowed.
- The ten required cases, verbatim: ADB not installed, ADB not found, Device offline, Device unauthorized, Connection refused, Wireless debugging disabled, Termux unavailable, Hermes process not found, Command failed, Network timeout.
- Baseline gates that must stay green: 360 Rust tests, 197 frontend tests, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, `npm run lint`, `npm run typecheck`.
- CI runs tests on Windows too: no shell-script subprocess fixtures; all reproduction tests must use the in-process `FakeRunner` or portable APIs.
- Do not touch `deny.toml`, advisory ignores, or dependencies — this task changes no manifests.
- New tests belong beside existing tests for the module under test (inline `#[cfg(test)] mod tests`), following the file's existing style.
- Progress entries are dated **2026-10-26** and appended under `### [~] M10-T1 Error audit` in `docs/tasks/M10-production.md`; `docs/TODO.md` is synced.
- Commit messages follow repo convention: `test(M10-T1): …` / `fix(M10-T1): …`.

## Key facts for implementers (verified 2026-10-26)

- `AppError` lives in `src-tauri/src/error.rs`; `err.to_payload() -> ErrorPayload` where `ErrorPayload { message: String, details: Option<String>, … }` (`error.rs:33,188`).
- Every §16 case already has an `AppError` variant: `AdbNotFound { searched }`, `DeviceOffline { serial }`, `DeviceUnauthorized { serial }`, `ConnectionRefused { address, output }`, `WirelessDebuggingDisabled { address, output }`, `TermuxUnavailable { reason }`, `HermesNotFound`, `CommandFailed { … }`, `Timeout { operation, after_ms }`, plus `DeviceNotFound`, `Config`, `Io` (`error.rs:39-77`).
- `error.rs` tests already assert **constructed** payloads for all ten categories (the loop at `error.rs:316-326`). What is missing — and what this plan adds — is reproduction through each error's **generation path** (real classifier/transport code fed with real-world output strings).
- Process injection: `FakeRunner::new().on(argv_key, Ok(RawOutput { stdout, stderr, exit_code, duration_ms }))` where `argv_key` is `args.join(" ")` **without the program path and without `-s <serial>`** (see `process.rs` FakeRunner::run: it keys on `args` only). Mirror argv keys from existing tests in the same file before writing new ones.
- `AdbClient::new(path: impl Into<PathBuf>, runner: Arc<dyn ProcessRunner>)` (`client.rs:84`); `client.connect(address) -> AppResult<ConnectOutcome>` (`client.rs:144`), `client.devices()` (`client.rs:114`), `client.shell(...)` (`client.rs:244`).
- `TermuxUnavailable` is generated at `src-tauri/src/termux/ssh.rs:53`. `HermesNotFound` generation sites: find with `rg -n "HermesNotFound" src-tauri/src --type rust` (variants/messages at `error.rs:62-63,129,171`; construction elsewhere).
- UI: `src/components/ErrorPanel.tsx` renders `error.message`, a `Details`/`Hide details` toggle button with `aria-expanded`, and a `<pre>` with `error.details`. Existing tests: `src/components/components.test.tsx` (`ErrorPanel` describe at line 8).
- The 2026-10-13 audit baseline (24 sites, all documented) is recorded at `docs/tasks/M10-production.md:23-31` — use it to diff what is genuinely new.

---

### Task 1: Post-merge panic/swallow re-audit

**Files:**
- Modify: production files surfaced by the inventory commands (known-new-since-baseline candidates: `src-tauri/src/config.rs`, `src-tauri/src/devices.rs`, `src-tauri/src/lifecycle.rs`, `src-tauri/src/logs/logcat.rs`, `src-tauri/src/hermes/env.rs`, `src-tauri/src/provision/executor.rs`, `src-tauri/src/commands/device.rs`, `src-tauri/src/commands/logs.rs`, `src-tauri/src/state.rs`, `src-tauri/src/monitor.rs`)
- Modify: `docs/tasks/M10-production.md` (append classification table under M10-T1)
- Test: inline `#[cfg(test)]` tests in whichever module receives a fix

**Interfaces:**
- Consumes: `AppError` variants and `AppResult<T>` from `src-tauri/src/error.rs` for any panic→error conversion.
- Produces: an updated classification table in the task doc; no API changes.

**Decision rules (apply verbatim, in order):**

1. Site inside `#[cfg(test)] mod tests` or a `#[cfg(target_os = "…")]` test block → **out of scope** (production audit). Record as `test`.
2. Site already listed in the 2026-10-13 baseline (`docs/tasks/M10-production.md:23-31`) **and** its justification comment is still present → record as `documented`. If the comment was lost in a refactor, restore it.
3. Production `.lock().unwrap()` over a short critical section containing only map/vec operations (no I/O, no calls that can panic) → keep; add the justification comment if missing.
4. Production `.unwrap()`/`.expect()` that can fail on I/O, device/user input, parsing, or any result not proven infallible → **replace with structured `AppError` propagation** (TDD: failing test first, then fix).
5. `let _ = …` / `if let Err(…) = …` that hides a user-visible failure → propagate or log with context + inline comment. Benign intentional drops (send on a closed channel during teardown, best-effort cleanup after a primary failure already reported) → keep, add inline comment stating why ignoring is safe.

- [ ] **Step 1: Run the inventory**

```bash
rg -n "\.unwrap\(\)|\.expect\(" src-tauri/src -t rust
rg -n "let _ = " src-tauri/src -t rust
rg -n "if let Err\(" src-tauri/src -t rust
```

- [ ] **Step 2: Classify every hit** against the five rules; build the table: `| File:line | Site (short code) | Category: test / documented / infallible-commented / NEW-fix / NEW-document |`. Confirm each baseline site still has its comment (rule 2).

- [ ] **Step 3: For each NEW-fix site — write the failing test first**

Pattern (adapt to the module's existing test style):

```rust
#[test]
fn poisoned_thing_returns_error_instead_of_panicking() {
    // arrange the failure condition the old unwrap() would have panicked on
    let result = exercise_the_path();
    assert!(matches!(result, Err(AppError::Io(_) | AppError::Config(_))));
}
```

Run: `cargo test <test_name>` — Expected: FAIL (panic/error mismatch).

- [ ] **Step 4: Convert the site** to `AppResult` propagation (or `.expect("justification")` + comment if the site proves genuinely infallible under rule 3).

- [ ] **Step 5: Re-run the test** — Expected: PASS. Then `cargo test` — Expected: 360+ pass, 0 failed.

- [ ] **Step 6: Append the classification table + one-paragraph summary to `docs/tasks/M10-production.md`** under M10-T1 as `- **Progress (2026-10-26):** …`, stating the sweep date, total sites, categories, and fixes made.

- [ ] **Step 7: Commit**

```bash
git add -A src-tauri/src docs/tasks/M10-production.md
git commit -m "test(M10-T1): post-merge panic/swallow re-audit (2026-10-26 sweep)"
```

---

### Task 2: Reproduce brief §16 cases through their generation paths

**Files:**
- Modify: `src-tauri/src/error.rs` (add shared payload assertion helper in its `#[cfg(test)]` tests)
- Modify: tests beside each generation site — start from `src-tauri/src/adb/client.rs`, `src-tauri/src/adb/locate.rs`, `src-tauri/src/termux/ssh.rs`, plus modules found by the Step-1 commands for `HermesNotFound`, `CommandFailed`, `Timeout`
- Modify: `docs/tasks/M10-production.md` (reproduction matrix)

**Interfaces:**
- Consumes: `AdbClient::new(path, runner)`, `FakeRunner::new().on(argv, AppResult<RawOutput>)`, `RawOutput { stdout, stderr, exit_code, duration_ms }`, `AppError::to_payload()`.
- Produces: `#[cfg(test)] pub(crate) fn assert_brief_payload(err: AppError, needle: &str)` in `error.rs` tests, reused by every case test below; a 10-row reproduction matrix in the task doc.

- [ ] **Step 1: Locate every generation site**

```bash
rg -n "AppError::(AdbNotFound|DeviceOffline|DeviceUnauthorized|ConnectionRefused|WirelessDebuggingDisabled|TermuxUnavailable|HermesNotFound|CommandFailed|Timeout)" src-tauri/src -t rust
rg -n "pub fn" src-tauri/src/adb/locate.rs
```

Record the file:line of each construction and the existing test (if any) that already drives it — extend those tests rather than duplicating fixtures.

- [ ] **Step 2: Add the shared helper to `error.rs` tests**

```rust
pub(crate) fn assert_brief_payload(err: AppError, needle: &str) {
    let payload = err.to_payload();
    assert!(!payload.message.trim().is_empty(), "empty human message for {err:?}");
    assert!(
        payload.message.contains(needle),
        "message {:?} lacks human-readable marker {needle:?}",
        payload.message
    );
    let details = payload.details.expect("brief §16 requires expandable Details");
    assert!(!details.trim().is_empty(), "empty details for {err:?}");
}
```

Run: `cargo test -p hermes-control-center payload` (adjust `-p` to the real package name from `src-tauri/Cargo.toml`) — Expected: existing payload tests still PASS (helper compiles).

- [ ] **Step 3: Case-by-case reproduction** — for each row below: read the classifier/generation code first, feed it the literal output it matches (string columns below are the canonical real-world texts; if the matcher expects a variant spelling, use the matcher's own literal), assert with `assert_brief_payload(err, "<needle>")`. Extend the existing test if one produces the error already.

| # | Case | Variant | Driver (from Key facts) | Needle |
|---|------|---------|------------------------|--------|
| 1 | ADB not installed | `AdbNotFound` | drive the public locate API in `adb/locate.rs` with a configured path that does not exist (`AdbClient::new("/definitely/missing/adb", …)` or locate's own entry fn) | `"ADB"` |
| 2 | ADB not found | `AdbNotFound { searched }` | same path with search dirs that contain no adb; also assert `!searched.is_empty()` | `"ADB"` |
| 3 | Device offline | `DeviceOffline` | `FakeRunner` response whose stderr/stdout contains `error: device offline`, through `client.shell`/`client.devices` (`client.rs:114,244`) | `"offline"` |
| 4 | Device unauthorized | `DeviceUnauthorized` | `devices -l` line: `<serial>\tunauthorized` (see `adb/parse.rs` state parsing) | `"unauthorized"` |
| 5 | Connection refused | `ConnectionRefused` | `client.connect("192.168.0.107:5555")` with stdout `failed to connect to '192.168.0.107:5555': Connection refused` — mirror the argv key from the existing connect tests (`client.rs:399+`) | `"refused"` |
| 6 | Wireless debugging disabled | `WirelessDebuggingDisabled` | read the matcher arms that construct this variant (`rg WirelessDebuggingDisabled src-tauri/src`), feed that literal through `connect` | `"wireless"` or the matcher's own human word |
| 7 | Termux unavailable | `TermuxUnavailable` | drive the check at `termux/ssh.rs:53` (read ~30 lines above it for the trigger). If the trigger requires a live SSH endpoint, reach for a localhost address with nothing listening; if the path is unreachable without a phone, document precisely why (Step 5) | `"Termux"` |
| 8 | Hermes not found | `HermesNotFound` | drive the construction site found in Step 1 (Hermes process detection with fake runner output showing no process) | `"Hermes"` |
| 9 | Command failed | `CommandFailed` | non-zero `exit_code` + stderr through the transport/execute classifier (`transport/adb_shell.rs` or the site from Step 1) | message's command-failure words (read the `error.rs` Display impl, ~line 65) |
| 10 | Network timeout | `Timeout` | find the construction site from Step 1; if it wraps a future in `tokio::time::timeout`, call it with a near-zero deadline and a stalled fake; else feed the runner response that the wrapper classifies as a timeout | the `operation` name or `"timed out"` (check `error.rs` Display, ~line 71) |

- [ ] **Step 4: Run the full suite** — `cargo test` Expected: all pass (baseline 360 + new cases), `cargo clippy --all-targets -- -D warnings` clean, `cargo fmt --check` clean.

- [ ] **Step 5: Build the reproduction matrix in `docs/tasks/M10-production.md`** — append under M10-T1: `- **Progress (2026-10-26):**` with a 10-row table `| Case | Reproduced locally via | Evidence (test name) | Live e2e status |`. For any case Step 3 documented as unreachable locally (expected: parts of 7/8, possibly wireless-debugging-disabled e2e on a real radio), set Live e2e status to `blocked: requires connected phone (live failure matrix)`.

- [ ] **Step 6: Commit**

```bash
git add -A src-tauri/src docs/tasks/M10-production.md
git commit -m "test(M10-T1): reproduce brief §16 failure cases through generation paths"
```

---

### Task 3: ErrorPanel Details-expander rendering verification

**Files:**
- Modify (test only, if gaps found): `src/components/components.test.tsx`

**Interfaces:**
- Consumes: `ErrorPanel { error: ErrorPayload; onRetry?: () => void }` (`src/components/ErrorPanel.tsx`).
- Produces: none (verification test).

- [ ] **Step 1: Inspect existing coverage**

Run: `npx vitest run src/components/components.test.tsx`

Read the `ErrorPanel` describe block (line 8+). Inventory which brief-UI assertions exist: (a) human message rendered, (b) `Details` button present when `details` non-null, (c) clicking toggles `aria-expanded` and reveals a `<pre>` containing the technical details, (d) no `Details` button when `details` is null.

- [ ] **Step 2: Write failing tests for any gap** (TDD — most likely (c) is missing)

```tsx
it("expands technical details behind the Details button", async () => {
  render(
    <ErrorPanel error={{ kind: "adb", message: "Unable to connect to Android device.", details: "ADB returned:\ndevice offline" }} />,
  );
  const toggle = screen.getByRole("button", { name: "Details" });
  expect(toggle).toHaveAttribute("aria-expanded", "false");
  await userEvent.click(toggle);
  expect(screen.getByText(/ADB returned:/)).toBeInTheDocument();
  expect(toggle).toHaveAttribute("aria-expanded", "true");
});
```

(Use the project's existing test utilities in that file — same imports as sibling tests.)

- [ ] **Step 3: Run** `npx vitest run src/components/components.test.tsx` — Expected: new test FAILS if the gap was real; component code should already support all four assertions — if it does, the test passes immediately (coverage confirmation, no component change).

- [ ] **Step 4: Run full frontend gates** — `npm run lint && npm run typecheck && npm test -- --run` — Expected: all green (197 + new).

- [ ] **Step 5: Commit**

```bash
git add src/components/components.test.tsx
git commit -m "test(M10-T1): verify ErrorPanel exposes expandable Details per brief §16"
```

---

### Task 4: Docs sync, full gates, closeout

**Files:**
- Modify: `docs/tasks/M10-production.md` (M10-T1 status heading + closeout entry)
- Modify: `docs/TODO.md` (In Progress M10-T1 line, Recently Completed)

**Interfaces:**
- Consumes: Tasks 1–3 progress entries and matrices.
- Produces: updated status snapshots; no code.

- [ ] **Step 1: Reconcile the M10-T1 section** in `docs/tasks/M10-production.md`:
  - Update the two residual "remain open" bullets (`:10`/`:11` style) to point at the 2026-10-26 entries: the non-test panic/error-propagation grep audit is now re-verified post-merge; local §16 reproduction is complete for every locally drivable case.
  - Change the heading `### [~] M10-T1 Error audit` to `### [x] M10-T1 Error audit` **only if** every remaining open item is captured as an explicitly blocked live-matrix row (phone-gated e2e). If any local item remains, keep `[~]` and say what remains.
- [ ] **Step 2: Sync `docs/TODO.md`** — update the `[~] M10-T1` line (line ~98) to match; if closing, move a one-line summary to Recently Completed with the evidence (test counts + matrices).
- [ ] **Step 3: Run all gates**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
npm run lint && npm run typecheck && npm test -- --run && npm run format:check
```

Expected: everything green; if `src/types/generated/` churns from the test run, `git checkout -- src/types/generated/` (known ts-rs formatting noise, schema unchanged).

- [ ] **Step 4: Commit**

```bash
git add docs/tasks/M10-production.md docs/TODO.md
git commit -m "docs(M10-T1): record 2026-10-26 audit sweep and §16 reproduction matrix"
```

---

## Self-review notes

- Spec coverage: brief §16 ten cases → Task 2 matrix (all ten rows); "never silently swallow" + unwrap grep audit → Task 1; "expandable Details" UI → Task 3; live/phone-only residue → Task 2 Step 5 + Task 4 closeout rules. Existing export-path, mutex-poison, classification, and payload-format slices from prior progress entries are untouched (already closed).
- No placeholder steps: every step has an exact command, literal fixture string, or concrete code block. Task 2 rows intentionally direct the implementer to the matcher's own literals — the classifier is the source of truth for fixture text, and the read-command is given.
- Type consistency: `assert_brief_payload(err: AppError, needle: &str)` defined in Task 2 Step 2, used in Step 3 only; `AdbClient::new(path, runner)` / `FakeRunner::on(argv, AppResult<RawOutput>)` signatures copied from source.
