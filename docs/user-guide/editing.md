# Editing

> Read when writing, formatting, using source mode, or pasting content.

Canonical content is ParchMint Markdown. Use documented syntax for headings,
emphasis, lists, links, images, alignment, thematic breaks, page breaks, and
named styles. Style references use stable IDs; page breaks are compile markers,
not live page layout.

Each pane can retain multiple document tabs. The thin formatting strip above
the panes is shared and always applies to the active tab in the focused pane;
each tab otherwise retains its own cursor, selection, scroll, and text undo.
Raw Markdown source buffers are tab-local and remain available when switching
tabs. If the same document changes in another pane, the older source buffer is
not allowed to overwrite it. Finish or discard every source buffer before
saving, exporting, changing projects, or quitting.

## Safety rules

- Save/reopen preserves supported syntax deterministically.
- Unknown front-matter fields and unsupported blocks remain visible source.
- Plain paste remains literal; rich paste keeps allowed formatting and removes active/remote content.
- A hard parse error keeps the source buffer available until fixed or explicitly discarded.
- Visual formatting is durable only when represented by the Markdown codec; Qt HTML is never canonical source.

See [ParchMint Markdown 1](../format/parchmint-markdown-1.md) for exact syntax.
