# Stage 08: fixtures, documentation, and regression matrix

Difficulty: easy but high volume  
Recommended model: GPT-5.6 Luna, medium or high reasoning  
Depends on: stages 1–7  
Master references: all implemented behavior in `PLAN.md`; this stage may not redefine it

## Outcome

Expand repetitive test breadth, example projects, support matrices, and documentation after the architecture and features are stable. This is a Luna stage because the work is large but bounded by existing behavior and specifications.

This stage does not excuse missing feature tests from earlier stages. It broadens combinations, platforms, fixtures, and documentation.

## Primary ownership

- `tests/fixtures`, `tests/golden`, and generated corpus manifests
- `crates/parchmint-test-support`
- Example projects
- User/developer documentation that describes already implemented behavior
- Regression matrix and manual test charters
- Translation catalogs populated from existing source strings

## Hard boundary

The agent must not:

- Change canonical schemas or Markdown grammar
- Change Qt/Rust ownership or threading
- Change recovery guarantees or path-security rules
- Invent exporter behavior not present in code
- Refactor production architecture merely to simplify a fixture
- Mark an observed product defect as “expected” without an existing specification/ADR

When a fixture exposes a nontrivial product defect, add the smallest failing regression test, document it in the handoff, and leave the architectural fix for stage 9 or a Terra/Sol agent.

## Required work

### 1. Format and Markdown fixture expansion

- Add small focused fixtures for every documented front-matter field, manifest construct, style property, and Markdown extension.
- Add pairwise combinations of paragraph styles, inline styles, alignment, lists, links, images, page breaks, Unicode, and opaque content.
- Add malformed/truncated/unknown-key/newer-version fixtures with expected diagnostics.
- Add repeated-save and cross-platform newline/path golden cases.
- Add migration fixture chains and recovery-journal examples for all existing versions.

### 2. Export matrix expansion

- Add golden inputs covering every row in the exporter support matrix.
- Add deterministic structural assertions for Markdown, text, HTML, PDF metadata/layout markers, EPUB package contents, and DOCX document/style XML.
- Add missing-asset, duplicate-name, unusual-Unicode, long-title, deep-list, and cancellation cases.
- Document intentional format degradation using the wording already accepted in stage 7.

### 3. Example and stress projects

- Create small “tour,” medium novel, research-heavy, Unicode, and format-edge-case example projects.
- Ensure every example is human-readable and contains no copyrighted or sensitive text.
- Complete deterministic corpus generators for 100, 1,000, and 10,000 nodes and configurable word counts.
- Store generator seeds/configuration instead of committing enormous generated corpora.

### 4. Documentation

- Expand developer bootstrap, architecture navigation, build/test command, format, recovery, backup, external-edit, and export documentation.
- Write user guides for binder operations, summary outlining, editor/styles, research/split panes, search, compile/export, backups, and recovery.
- Produce a complete keyboard shortcut reference from the command registry.
- Produce platform test charters for IME, accessibility, installers, file associations, sleep/resume, external changes, and crash recovery.
- Populate translation catalogs and flag layout/string expansion problems without translating through guesswork.

### 5. Regression matrix

- Create a traceability table from every master-plan requirement and acceptance gate to automated tests, manual charters, or an explicit stage 9 blocker.
- Parameterize repetitive tests across themes, panes, node kinds, formatting combinations, and supported platforms where the harness permits.
- Remove duplicate fixtures only when coverage remains obvious and named.

## Acceptance gate

- Every implemented persisted construct has a focused valid fixture and at least one relevant invalid/forward-compatibility case.
- Every exporter support-matrix row has automated or explicitly manual coverage.
- Corpus generators deterministically reproduce the documented stress sizes.
- User and developer docs match current commands and behavior.
- Every version 1 requirement maps to coverage or a named stage 9 blocker.
- Production format/architecture behavior is unchanged except for trivial, separately tested corrections.

## Handoff

Create `docs/handoffs/08-fixtures-documentation-and-regression-matrix.md`. Include added coverage counts/categories, generator commands/seeds, requirement traceability location, documentation gaps, and every failing regression intentionally handed to stage 9.

