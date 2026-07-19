# Stage 01: foundation and risk spikes

Difficulty: very hard  
Recommended model: GPT-5.6 Sol, high or max reasoning  
Cost-conscious fallback: GPT-5.6 Terra, max reasoning  
Depends on: nothing  
Master references: `PLAN.md` “Locked architecture,” “Content and project-format contract,” and “Cross-stage quality gates”

## Outcome

Retire the architectural risks that could invalidate the product before feature development begins, then turn the successful spike into a minimal production scaffold. At completion, subsequent agents must have a buildable Rust/Qt application, a proven rich-text interchange path, stable dependency directions, and recorded architectural decisions.

This stage is intentionally difficult and broad. Do not postpone a failed editor, Markdown, bridge, or platform assumption to a later feature stage.

## Primary ownership

- Root Cargo, CMake, toolchain, and task-runner files
- Initial `crates/*` workspace skeleton
- `app/cpp`, `app/qml`, and application resources
- `docs/architecture` ADRs
- Initial CI workflows and developer bootstrap documentation
- Spike fixtures under `tests/fixtures/spike`

## Required work

### 1. Toolchain and application bootstrap

- Pin a supported Rust stable toolchain and a specific supported Qt 6 minor version.
- Establish Cargo/CMake/CXX-Qt integration with reproducible developer commands.
- Create a minimal Qt Quick Controls application with the Material-inspired theme foundation.
- Prove Rust-to-QML properties, invokable commands, signals, error propagation, and a Rust-backed list model.
- Establish structured logging, top-level error reporting, debug assertions, and crash diagnostics hooks that do not transmit data.
- Add `just` commands for bootstrap, build, run, format, lint, test, and smoke packaging.

### 2. Rich-text editor spike

- Compare a QML `TextArea`/`QQuickTextDocument` adapter against an embedded Qt Widgets `QTextEdit` only as far as necessary to choose the production editor host.
- Exercise `QTextCursor`, `QTextBlockFormat`, `QTextCharFormat`, custom objects, selection state, and grouped undo actions.
- Round-trip a representative semantic fixture containing headings, paragraph and character styles, bold, italic, superscript, subscript, alignment, lists, links, images, opaque blocks, and page breaks.
- Verify paste from plain text and common rich clipboard formats.
- Verify dead keys, bidirectional text, grapheme navigation, and at least one non-Latin IME on every platform available to CI or the developer.
- Benchmark typing, selection, scrolling, formatting, load, and conversion with a generated 250,000-word section.
- Demonstrate two simultaneously open editor documents without shared selection or undo state.

### 3. Markdown and semantic-model spike

- Evaluate maintained Rust Markdown parsers that preserve source ranges and extension nodes.
- Define the boundary types passed between Rust and Qt. Do not pass raw `QTextDocument` internals into the domain layer.
- Demonstrate deterministic semantic serialization without `QTextDocument::toMarkdown()`.
- Preserve unknown YAML keys and an unsupported Markdown block through a load/save cycle.
- Decide how stable style IDs, page breaks, fenced attributes, opaque blocks, and attachment references are represented.
- Produce the initial ParchMint Markdown grammar ADR. Exact format implementation belongs to stage 2/3, but syntax uncertainty must be closed here.

### 4. Scale and infrastructure spikes

- Expose a lazy Rust-backed Qt tree model containing 10,000 nodes and benchmark initial display, expansion, scrolling, and updates.
- Prove an atomic same-directory write with flush and rename on all target platforms.
- Prove SQLite FTS5 creation, incremental update, deletion, and rebuild from source files.
- Establish background-task execution and a revision/generation mechanism that discards stale results.
- Verify the UI thread never waits for an indexing or filesystem scan in the spike.

### 5. ADRs

Record accepted decisions for:

- Qt version, linking posture, modules, and initial licensing constraints
- QML editor versus hosted Widgets editor
- CXX-Qt ownership, threading, cancellation, and error boundaries
- Rust Markdown parser and semantic AST strategy
- ParchMint Markdown extension syntax
- Atomic-write and recovery direction
- SQLite/FTS choice and rebuildability
- Testing framework and cross-platform CI matrix

## Acceptance gate

- A clean checkout builds and opens the same native application on Windows, macOS, and Linux CI.
- The supported formatting spike round-trips without silent semantic loss.
- Unknown front matter and the chosen opaque-block fixture survive unchanged.
- The 250,000-word editor and 10,000-node tree demonstrate a credible path to the master performance budgets; measurements are recorded.
- Background indexing and filesystem operations do not block the UI event loop.
- All listed ADRs are accepted and no critical architectural question is deferred without a measured fallback.
- `just format-check`, `just lint`, `just test`, and `just build` pass.

## Out of scope

- Full project schemas and migrations
- Production binder, outline, search, or export UI
- Production autosave/recovery
- Complete user-facing styling workflow
- Release installers or signing

## Handoff

Create `docs/handoffs/01-foundation-and-risk-spikes.md`. Include benchmark hardware, exact Qt/Rust versions, chosen editor host, rejected alternatives, known platform limitations, and the stable commands stage 2 should use.
