# ParchMint Future Work

**Status:** Explicitly deferred capabilities and extension constraints  
**Version:** 1.3  
**Date:** 2026-07-31

## 1. How to use this document

Nothing here is v1 scope unless promoted through direct updates to the product specification, design, architecture, implementation plan, and acceptance plan.

The v1 architecture keeps credible extension paths, but agents must not build speculative features or broaden contracts without a concrete promoted requirement.

## 2. Editor and workspace

- Recursive left/right/top/bottom editor groups.
- Top/bottom companion orientation.
- Group copy and keyboard cut.
- Cross-project copy with style/metadata/comment mapping.
- Quick document switcher and back/forward navigation.
- Focus/typewriter modes.
- Visible whitespace and per-view zoom.
- Transparent editor optimizations that preserve full behavior.
- Alternative GUI/editor implementing the same canonical and application/editor contracts.

A feature-reduced or segmented large-document mode requires an explicit future product decision and is not implied by transparent optimization.

## 3. Appearance and preferences

- Toolbar/status-bar appearance quick toggle.
- Additional themes beyond Light/Dark.
- User-authored themes.
- Density choices.
- Roaming preferences.
- Per-project appearance overrides.

Appearance must remain separate from authored document style/export semantics.

## 4. Import and Research resources

- Plain text, Markdown, HTML, DOCX, and hierarchy import.
- Research note import/conversion.
- Text/Markdown/image/PDF preview.
- Arbitrary attachments, bookmarks, snapshots.
- Copy-into-project and link-in-place policies.
- PDF/image annotations.

Future imports should use `Importer`/`ContentHandler`, preview a plan, and apply as one project-undo/history operation.

## 5. Cards and planning

- Multi-column corkboard.
- Saved filters/views.
- Typed metadata such as status, POV, dates, tags, target word count.
- Timeline/relationship views.
- Independent planning arrangements that do not silently alter manuscript order.

## 6. Rich text and review

- Footnotes/endnotes.
- Tables and embedded images.
- Arbitrary highlights/colors and per-selection fonts/sizes.
- Drop caps, columns, sections, page numbering, headers/footers.
- Track changes and review display modes.
- Snapshot comparisons.
- Search comments and project-wide All Comments.
- Comment authors/asynchronous review exchange.

## 7. Search and analysis

- User-selectable Manuscript/Research/subtree search scopes.
- Regex search/replace.
- Saved searches.
- Replacement in Synopsis/metadata.
- Structural queries.
- Language-aware stemming/diacritic options.
- Repetition, sentence, style, and readability analysis.

Search backend replacement remains possible through `SearchIndex`; indexes remain disposable.

## 8. Word counts and goals

- Group totals.
- Research totals.
- Whole-project totals distinct from Manuscript total.
- Target word counts, sessions, goals, progress analytics.
- Historical writing statistics.

## 9. Spellcheck and writing aids

- Per-document language override.
- Mixed-language spans.
- Grammar checking.
- Context-sensitive or semantic writing suggestions.
- Smart quotes and language-aware punctuation.
- Automatic dash/ellipsis rules.
- Special-character palette.

Future language features continue through ParchMint-owned contracts and require explicit privacy, offline/network, licensing, performance, and cross-platform decisions.

## 10. Export and publishing

- Partial selected group/document scopes.
- Per-node inclusion overrides.
- Generated TOC and in-app preview.
- DOCX, EPUB, PDF, Markdown, plain text, LaTeX.
- Submission/print-ready templates, saved profiles, cover/front/back matter.

Each new target implements `Exporter` over the neutral export plan.

## 11. History, backup, and maintenance

- Partial checkpoint restore for document/group/subtree.
- Remote backup/restore and multiple destinations.
- Integrity repair UI and history size reporting.
- Optional compaction/pruning only after an explicit product decision.

Current requirements exclude automatic pruning, permanent purge, Duplicate Project, Archive Project, and general Git UI.

## 12. Templates

- Project templates.
- Style templates.
- Metadata-field templates.
- Export-profile templates.

Templates generate fresh IDs and no copied history.

## 13. Distribution and platform

- Automatic updates.
- App-store distribution.
- Additional CPU architectures.
- Expanded Linux packages, including AppImage only after runtime compatibility is proven.
- Automatic workspace reopening.
- Lower production MSRV/toolchain where feasible.

## 14. Collaboration and other clients

- Asynchronous review packages before real-time collaboration.
- Real-time collaboration with identity, permissions, presence, CRDT/OT, offline reconciliation, and history integration.
- Mobile/web clients after a separate product/architecture review.
- AI-assisted writing only after explicit privacy, provenance, offline/network, and user-control decisions.

## 15. Promotion checklist

Before promoting a feature:

1. Add normative workflows/requirement IDs to the product specification.
2. Update Penpot screens/components/states.
3. Define canonical/migration impact.
4. Confirm port/state-owner changes.
5. Define project/document undo, save, recovery, history, search, export, spellcheck, appearance, and accessibility semantics as applicable.
6. Add scale/performance budgets.
7. Add cross-platform tests.
8. Update implementation and acceptance plans.
9. Remove the item from this future-work document.

## 16. Features not to infer

Do not infer raw HTML editing, user-facing Git, permanent purge/history erasure, automatic pruning, Duplicate/Archive Project, real-time collaboration, mobile/web, AI generation, automatic external rich-text merge, or a proprietary binary database as the sole current-content store.
