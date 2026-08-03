# ParchMint Future Work

**Status:** Non-v1 roadmap; not an implementation source of truth
**Date:** 2026-08-02

The product specification's Included and Explicitly deferred sections define the complete v1 boundary. This file preserves likely extension directions and the constraints that should survive future planning; it does not authorize implementation work.

## Likely directions

- **Workspace and planning:** recursive pane layouts, alternative Cards arrangements, saved views, richer metadata, goals, and writing analytics.
- **Import and Research:** text/Markdown/HTML/DOCX import, managed attachments, images/PDFs, previews, and annotations.
- **Editing and review:** footnotes, tables, embedded media, richer styling, track changes, comment search, and review exchange.
- **Search and language:** scoped or regex search, saved/structural queries, per-document or mixed-language spellcheck, grammar, and writing analysis.
- **Export and publishing:** partial export, generated contents, additional document/ebook/print formats, profiles, and front/back matter.
- **History and backup:** partial checkpoint restore, remote backup, integrity tools, and explicitly governed retention or compaction.
- **Preferences and distribution:** additional/user themes, density, roaming preferences, automatic updates, app stores, architectures, and package formats.
- **Collaboration and clients:** review packages before real-time collaboration; mobile, web, and AI assistance only after separate privacy, ownership, offline/network, and architecture decisions.

## Persistent extension constraints

- Preserve open deterministic authored data and ParchMint-owned contracts.
- Keep imported or generated work inside project-command, undo, save, recovery, and History semantics.
- Keep indexes, caches, and analysis output disposable unless a future specification explicitly promotes them to authored data.
- Do not infer a feature-reduced large-document mode from transparent optimization.
- Keep appearance separate from authored prose styles and export output.
- Require explicit privacy, licensing, performance, accessibility, and cross-platform decisions for every new engine, service, format, or bundled resource.

## Promotion checklist

Before implementing an item:

1. Add normative behavior and stable requirement IDs to the product specification.
2. Update the current Penpot design and approved handoff where UI is affected.
3. Define canonical format/migration, state owner, public-port, undo, save, recovery, History, search, export, privacy, and security effects.
4. Add scale, performance, accessibility, native-platform, and release evidence.
5. Update the architecture, implementation plan, acceptance plan, and workflow tasks directly.
6. Remove the promoted item from this file so it does not compete with current scope.
