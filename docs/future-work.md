# Future work

Ideas to revisit when a concrete writing workflow justifies them. Remove completed
items and keep implementation details in the owning crate's documentation.
Keep the UI minimal: reveal detail when needed and avoid redundant copy or controls.

- Quick Open, a small command palette, Explorer filtering, and a document outline.
- Back/Forward through writing locations, separate from project History; optional
  return notes and summaries of writing sessions without productivity scoring.
- Keyboard operation of all dropdowns and screen-reader validation on each desktop
  platform; subtle comment markers with the Inspector hidden.
- Import text, Markdown, HTML, and DOCX; Research attachments and previews.
- Footnotes, tables, media, track changes, comment search, and review exchange.
- Scoped/regex search, more spellcheck languages, mixed-language text, CJK input,
  bidirectional editing, and optional grammar assistance.
- Export presets, partial export, contents/front matter, and ebook/print formats.
- Partial History restore, remote backup, integrity tools, and deliberate retention.
- Saved workspace arrangements, richer planning views, themes, and density choices;
  settings search when the inventory warrants it.
- Additional architectures, signing/notarization, and automatic updates. Review exchange should precede
  live collaboration; mobile, web, and AI assistance need separate product decisions.
- Short overlay fades that respect reduced motion and never delay input.

Keep authored data open and deterministic. Imports and generated edits must use
the command, undo, save, recovery, and History paths; caches stay disposable. New
services need explicit privacy and offline behavior. Backwards compatibility is
not required; add migrations only for an actual supported use.
