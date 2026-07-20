# Stage 11: Markdown and export integrity

Difficulty: hard  
Recommended model: GPT-5.6 Terra, high or xhigh reasoning  
Escalate to Sol when: inline parsing or cross-format fidelity requires a new semantic representation  
Depends on: stages 1–10  
Master references: `PLAN.md` “Content and project-format contract” and “Data-integrity, security, and privacy contract”; `plans/10-audit-report.md` §§4.1–4.2 and 7 P0.2/P0.4

## Outcome

Make every supported edit/save cycle and every advertised export format semantically stable. Unsupported input must remain source-backed and visible, malformed or adversarial input must fail within explicit resource limits, and a completed export must never contain known broken references or silently replace a destination from stale work.

The audit remediation already fixed the portable-path traversal hole, code-block closing fences, alignment-div spacing, benign CRLF/empty/EOF front matter, false attribute stripping for plain `{x}`, comparison text being treated as HTML, EPUB asset paths/navigation, and several Markdown-export escaping defects. Preserve those regressions and build the complete fidelity matrix around them.

## Primary ownership

- `crates/parchmint-markdown`
- `crates/parchmint-compile`
- Export orchestration in `crates/parchmint-app` and `crates/parchmint-bridge`
- Markdown/export fixtures, validators, fuzz targets, and support documentation
- Minimal Qt PDF boundary under `crates/parchmint-bridge/src/pdf_renderer.*`

## Required work

### 1. Escape-aware, bounded Markdown codec

- Replace delimiter searches with an escape-aware inline parser that correctly nests emphasis/strong/style/link/super/subscript nodes.
- Decode exactly the escapes the serializer emits for text, labels, destinations, titles, and attributes; prove repeated edit/save cycles reach a fixed point after one serialization.
- Parse link/image destinations and titles without changing percent-encoded identity, support escaped brackets/parentheses, and preserve reference links as opaque source until they have a lossless semantic representation.
- Implement CommonMark-compatible variable-length code spans and fenced code blocks, including content containing backtick runs.
- Represent nested and continued lists without flattening; preserve ordered-list starts and list boundaries.
- Gate trailing attributes on a fully valid attribute grammar. Reject malformed quoting instead of partially consuming user text.
- Add explicit document-byte, block-count, inline-depth, delimiter-scan, and diagnostic-count limits. Return typed errors; do not recurse on attacker-controlled depth.
- Remove the redundant full pulldown-cmark validation pass or make it the single parser of record where it can supply the required source mapping.

### 2. Edit-then-reparse coverage

- Build a table-driven test that marks every supported block and inline node changed, serializes it, reparses it, and compares semantic ASTs.
- Add pairwise nesting tests for emphasis, links, styles, code, super/subscript, hard/soft breaks, attributes, and alignment children.
- Add fixed-point properties for backslashes, percent escapes, quotes, braces, comparison operators, CRLF, and files without final newlines.
- Promote the audit reproductions into permanent fixtures and add fuzz assertions for bounded runtime/no panic/no semantic drift.

### 3. Export fidelity

- Route PDF through the compiled Qt `QTextDocument`/`QPdfWriter` renderer with Unicode shaping, semantic styles, images, margins, and page breaks. Keep the portable fallback explicit and non-default.
- Give each DOCX ordered list a distinct numbering instance; retain nested inline formatting; emit one paragraph style; map supported image MIME types accurately and visibly degrade unsupported ones.
- Use collision-resistant Markdown fences, valid attribute/title quoting, and semantic nested-list rendering in combined Markdown export.
- Validate every EPUB `src`/`href` against an archive member or allowed external URI, including percent-decoding and fragment targets.
- Replace the saturating ZIP fields with checked limits and ZIP64 support or a maintained writer. Never emit a corrupt archive above 4 GiB.

### 4. Destination transaction and stale work

- Render and validate into a destination-adjacent temporary artifact without touching the requested path.
- Re-check project generation/revision and collision policy on the UI owner immediately before commit, then atomically install the artifact.
- Make `CollisionPolicy::ReplaceFile` reachable only after explicit overwrite confirmation; enforce fail-if-exists without a TOCTOU overwrite window.
- Cancel only superseded content-affecting work. Selection, filtering, focus, and other projection-only commands must not cancel an export.
- Thread progress and cooperative cancellation through document traversal and renderer loops, not only between whole documents.

## Verification

- Run the full codec fixture suite plus property/fuzz tests with fixed seeds recorded in the handoff.
- Open generated PDF/EPUB/DOCX artifacts with structural validators and at least one real consumer per supported desktop platform.
- Add race tests for destination appearance, overwrite confirmation, cancellation immediately before commit, and stale revision completion.
- Assert warnings are bounded and deduplicated before allocation grows with document length.

## Acceptance gate

- Every supported construct survives 20 changed parse/serialize cycles with an equivalent semantic AST.
- Malformed/adversarial inputs stop within documented byte/depth/time limits without stack overflow or process abort.
- PDF preserves Unicode and required styling through the Qt renderer; EPUB and DOCX references/parts validate and open in the platform matrix.
- A cancelled, failed, collided, or stale export never changes the requested destination.
- No known audit reproduction in D5–D8 or the export-correctness list remains unfixed or untested.

## Out of scope

- Autosave/recovery and external-change UX (stage 12)
- Making currently unreachable editor/planning features visible (stage 13)
- Project-wide persistence and model scaling (stage 14)

## Handoff

Create `docs/handoffs/11-markdown-and-export-integrity.md`. Include the grammar/limit decisions, semantic fixed-point evidence, exporter support deltas, destination transaction protocol, validator/consumer results, fuzz seeds, and deliberately unsupported constructs.
