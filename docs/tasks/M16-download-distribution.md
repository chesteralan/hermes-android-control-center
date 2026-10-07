# M16 — Public Download Distribution

**Goal:** Let people download authentic, verified desktop releases from a stable public location. **Depends on:** M10 release readiness, M12 cross-platform packaging/signing, and M15 documentation hub.

## Distribution Decision

Use GitHub Releases as the primary download host for the first public release. The existing release workflow already builds platform bundles, generates updater metadata and checksums, signs release assets, and uploads them to one draft GitHub Release. A maintainer publishes the draft after release QA.

Do not add a second host just to make downloads possible. Consider Cloudflare R2 only if a custom download domain, independent retention, or measured bandwidth/availability needs justify the extra storage, credentials, synchronization, and support surface. If approved, R2 mirrors the exact signed GitHub assets; it must not rebuild or re-sign them.

## Tasks

### [ ] M16-T1 Release publication prerequisites
- Resolve the strict dependency-policy gate and required security review; do not publish while the release gate is red.
- Complete required macOS, Windows, and Linux signing setup and platform install/update QA.
- Confirm the version, tag, release notes, support policy, and asset list with maintainers before creating a release.

### [ ] M16-T2 Verify the GitHub Release pipeline
- Trigger the existing tag-based release workflow with the approved version tag.
- Confirm all three platform jobs pass and the publisher creates a draft containing the expected installers, updater signatures, metadata, and SHA256SUMS.
- Verify updater URLs resolve to the exact published asset names, and validate Authenticode, GPG, AppImage, and updater signatures as applicable.
- Publish the draft only after a maintainer reviews the assets and release notes.

### [ ] M16-T3 Public download path and instructions
- Link the README's download action to the latest published stable GitHub Release; keep prereleases clearly labeled and separate.
- List supported OS/architectures, package formats, minimum OS versions, install/update guidance, and checksum/signature verification steps.
- Explain that Linux deb/rpm updates use the package manager and are not installed by the AppImage updater.

### [ ] M16-T4 Download and installation acceptance
- From a browser session without GitHub authentication, download each supported artifact and its verification files.
- Verify checksums/signatures and complete fresh-install and upgrade tests on the supported OS matrix.
- Confirm release assets remain available from the documented public URL and that rollback to the previous release is documented.

### [ ] M16-T5 Optional Cloudflare R2 mirror evaluation
- Only proceed if maintainers identify a concrete need for a custom domain, independent retention, or measured traffic capacity.
- Record expected storage/egress costs, URL/domain ownership, retention and rollback behavior, and incident ownership before implementation.
- If approved, use a narrowly scoped CI upload token and a public read-only bucket; mirror immutable, versioned release assets and verification files from the completed GitHub Release.
- Verify byte-for-byte parity, checksums, signatures, caching behavior, and rollback. Never make an unsigned or partially uploaded mirror the canonical download.

## Exit Check

- [ ] The approved GitHub Release is public and all documented artifacts are anonymously downloadable and verifiable.
- [ ] Clean install and upgrade acceptance passes on every supported desktop platform.
- [ ] README and release documentation point to the correct stable release and explain verification.
- [ ] Cloudflare R2 is either explicitly deferred with no extra hosting credentials, or its mirror has passed parity and rollback checks.
