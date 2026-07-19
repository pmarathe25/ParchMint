# Stage 07: compile and export

Difficulty: hard  
Recommended model: GPT-5.6 Terra, high or xhigh reasoning  
Escalate to Sol when: cross-format style fidelity or Qt/Rust rendering boundaries remain unresolved  
Depends on: stages 3–6  
Master references: `PLAN.md` “Product contract,” “Content and project-format contract,” “Domain and dependency contract,” and “Data-integrity, security, and privacy contract”

## Outcome

Implement deterministic manuscript compilation and the complete version 1 export sequence: Markdown, plain text, HTML, PDF, EPUB, and DOCX. Exporters consume a Qt-independent compile IR so canonical semantics and tests are not tied to the live editor.

## Primary ownership

- `crates/parchmint-compile`
- Compile/export use cases and bridge models
- Compile preset, preview, progress, and destination UI
- Export fixtures, validators, and golden outputs
- Minimal Qt PDF rendering adapter

## Required work

### 1. Compile model and traversal

- Implement compile presets with stable IDs, selected roots, inclusion rules, title behavior, separators, metadata, style mapping, page settings, and exporter settings.
- Traverse selected nodes in stable binder preorder and exclude research unless explicitly selected by a supported rule.
- Resolve inherited styles entirely in Rust.
- Insert semantic title, separator, scene-break, and page-break nodes without mutating source documents.
- Produce a format-neutral compile IR containing normalized blocks, inlines, assets, semantic roles, and source provenance.
- Make compile cancellation and progress reporting revision-aware.

### 2. Preview and error model

- Provide a compile preview showing ordered source nodes, exclusions, warnings, and approximate/final counts.
- Distinguish validation warnings, unsupported-format degradation, destination failures, and internal errors.
- Never overwrite an existing destination until the replacement export validates and completes.
- Keep temporary output in a validated destination-adjacent or secure temporary location.

### 3. Exporters in required order

1. Markdown: combined file and optional directory output with deterministic separators and semantic extensions.
2. Plain text: documented normalization and configurable separators.
3. HTML: semantic HTML5, generated CSS, safe relative/copied assets, and optional self-contained output where practical.
4. PDF: controlled conversion of compile IR to a fresh Qt document and `QPdfWriter`, including paper size, margins, headers/footers, styles, images, and page breaks.
5. EPUB: valid package/container, metadata, XHTML content, navigation, CSS, assets, and deterministic spine order.
6. DOCX: prove a maintained Rust writer supports required styles, lists, headings, links, images, Unicode, and breaks before committing. If it cannot, record an ADR for a narrowly contained alternative. Optional Pandoc integration must be explicit and never a silent core dependency.

### 4. Validation and determinism

- Add structural validators for HTML/EPUB/DOCX where maintained tooling exists.
- Normalize unstable timestamps/IDs in golden testing without corrupting real metadata.
- Test missing assets, unsupported opaque nodes, invalid destinations, cancellation, full disk, and destination collisions.
- Ensure exporters produce actionable degradation warnings rather than silently dropping semantics.

## Acceptance gate

- Compile order and inclusion exactly match binder and preset rules across nested/multi-root fixtures.
- All six formats pass their golden and structural validation suites on Windows, macOS, and Linux.
- Headings, named styles, bold/italic, super/subscript, alignment, lists, links, Unicode, images, and page breaks meet each format's documented support matrix.
- Export cancellation/failure leaves existing destinations intact and cleans safe temporary artifacts.
- Export runs off the UI thread and does not block editing.
- Compile presets survive restart and remain human-readable in the open project format.

## Out of scope

- Pixel-identical rendering across operating systems
- Full desktop-publishing controls
- Import from DOCX/EPUB/Scrivener
- Proprietary cloud conversion services

## Handoff

Create `docs/handoffs/07-compile-and-export.md`. Include compile IR schema/API, exporter support matrix, validation tools, deterministic-testing rules, platform/font differences, and any documented degradations.
