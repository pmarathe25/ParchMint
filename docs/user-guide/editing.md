# Editing

> Read when writing, formatting, using source mode, or pasting content.

Canonical content is ParchMint Markdown. Use documented syntax for headings,
emphasis, lists, links, images, alignment, thematic breaks, page breaks, and
named styles. Style references use stable IDs; page breaks are compile markers,
not live page layout.

## Safety rules

- Save/reopen preserves supported syntax deterministically.
- Unknown front-matter fields and unsupported blocks remain visible source.
- Plain paste remains literal; rich paste keeps allowed formatting and removes active/remote content.
- A hard parse error keeps the source buffer available until fixed or explicitly discarded.
- Visual formatting is durable only when represented by the Markdown codec; Qt HTML is never canonical source.

See [ParchMint Markdown 1](../format/parchmint-markdown-1.md) for exact syntax.
