# Future work

This document records deliberate product deferrals. These ideas are not v1
commitments and must be designed against real author workflows before they are
scheduled.

## Navigation and writing context

- A constrained Quick Open and command palette for documents and a small set
  of ParchMint actions. It must not duplicate Explorer or turn into an
  extension-style command marketplace.
- A document-local outline, breadcrumbs, or a compact keyboard-accessible
  "locus trail" for returning to a heading, search result, comment, or recent
  authoring location. Any Back/Forward behavior remains separate from durable
  project History.
- A named distraction-free mode. The existing explicit pane-focus action is
  the smaller immediate solution; a separate mode needs a clear restore and
  keyboard-exit contract.
- A launcher-level resume-writing action, optional one-line return note, and
  durable last-location restoration. This requires careful workspace-state,
  deletion, and privacy semantics.
- Explorer filtering beyond the existing whole-project Global Search.

## Review, recovery, and project awareness

- An optional read-only recovery-detail view before Recover/Discard. It must
  remain distinct from durable History and must not slow the normal recovery
  path.
- "Revision weather": an optional, text-first History summary of drafting
  periods, structural changes, and named milestones. It must not become a
  productivity score or chart dashboard.
- An optional comment "margin memory" treatment: subtle unresolved-anchor
  markers at the manuscript edge when Inspector is hidden.
- A brief noninteractive "scene compass" label while scrolling long documents,
  derived only from existing headings and scene breaks.

## Configuration, planning, and publishing

- Settings search and deep links once the settings inventory is large enough
  to justify them.
- Export presets and post-export Open/Reveal actions, while preserving the
  current entire-Manuscript export scope.
- A Cards "chapter breathing" density lens that adds space around structural
  milestones without becoming a corkboard, dashboard, or implicit status
  system.
- A separate review-oriented comment search, only if its semantics can remain
  clearly distinct from v1 Global Search, which intentionally excludes
  comments.

## Motion and accessibility

- A small, opt-in motion policy for transient overlays only. The writing
  workspace intentionally responds immediately today: it has no decorative
  layout or navigation animation. If overlay fades are introduced, they need a
  reduced-motion setting, an injectable clock for deterministic tests, and a
  short opacity-only transition that never delays input or changes layout.
