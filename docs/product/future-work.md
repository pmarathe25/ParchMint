# ParchMint Future Work

The product specification's [Included and Explicitly deferred sections](scope.md) define the complete v1 boundary. This file preserves likely extension directions and the constraints that should survive future planning; it does not authorize implementation work.

## Likely directions

- **Workspace and planning:** recursive pane layouts, alternative Cards arrangements, saved views, richer metadata, goals, and writing analytics.
- **Import and Research:** text/Markdown/HTML/DOCX import, managed attachments, images/PDFs, previews, and annotations.
- **Editing and review:** footnotes, tables, embedded media, richer styling, track changes, comment search, and review exchange.
- **Search and language:** scoped or regex search, saved/structural queries, additional spellcheck languages, per-document or mixed-language spellcheck, CJK IME, bidirectional and Arabic editing, grammar, and writing analysis.
- **Export and publishing:** partial export, generated contents, additional document/ebook/print formats, profiles, and front/back matter.
- **History and backup:** partial checkpoint restore, remote backup, integrity tools, and explicitly governed retention or compaction.
- **Preferences and distribution:** additional/user themes, density, roaming preferences, automatic updates, app stores, architectures, and package formats.
- **Collaboration and clients:** review packages before real-time collaboration; mobile, web, and AI assistance only after separate privacy, ownership, offline/network, and architecture decisions.
- **Accessibility:** screen-reader integration, reduced-motion preference
  integration, and formal assistive-technology validation across desktop
  platforms.

## Persistent extension constraints

- Preserve open deterministic authored data and ParchMint-owned contracts.
- Keep imported or generated work inside project-command, undo, save, recovery, and History semantics.
- Keep indexes, caches, and analysis output disposable unless a future specification explicitly promotes them to authored data.
- Do not infer a feature-reduced large-document mode from transparent optimization.
- Keep appearance separate from authored prose styles and export output.
- Require explicit privacy, licensing, performance, accessibility, and cross-platform decisions for every new engine, service, format, or bundled resource.
