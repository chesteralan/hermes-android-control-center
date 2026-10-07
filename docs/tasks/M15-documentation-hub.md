# M15 — Documentation Hub

**Goal:** Provide one responsive, searchable HTML entry point for project documentation. **Depends on:** M14 for roadmap ordering only; the page is independently shippable.

## Tasks

### [x] M15-T1 Documentation catalog
- Create `docs/index.html` as a self-contained static page that opens directly from disk.
- Group product, architecture, operations, guides, spikes, and milestone documents for quick scanning.
- Link to the existing project emblem and use only local relative paths for repository content.

### [x] M15-T2 Search and navigation
- Add client-side text search and category filters without external scripts, fonts, or services.
- Keep controls semantic and keyboard accessible; announce result counts and provide a clear empty state.
- Support narrow mobile viewports without horizontal page overflow.

### [x] M15-T3 Link and responsive QA
- Verify every document link resolves, including milestone entries whose scope is maintained in the roadmap.
- Test search, filtering, empty results, keyboard focus, and representative desktop/mobile layouts.
- Link the hub from `docs/README.md` and keep the catalog aligned with the docs directory.

## Exit check

- [x] `docs/index.html` works locally without a server, build step, or external dependency.
- [x] Search and category filters show the expected documents and an accessible empty state.
- [x] Every catalog link resolves to an existing file or roadmap section.
- [x] The page is readable and usable at desktop and mobile widths.