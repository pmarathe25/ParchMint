# ADR-0005: ParchMint Markdown extension syntax

Status: Accepted (Stage 01)

## Decision

Adopt the syntax in `docs/format/parchmint-markdown-1.md`: Pandoc-compatible
attributes with stable `style-id`, fenced div alignment, `<sup>`/`<sub>`, the
exact `<!-- parchmint:page-break -->` marker, `asset:<id>` attachment references,
and visible source-backed opaque blocks.

Unknown YAML keys and opaque source survive until an explicit user conversion or
edit. Display names are never identity. Attribute serialization is deterministic.
The representative Stage 01 fixture is the minimum golden compatibility case.
