# Reference Fixtures

Fixtures describe reproducible project/document state behind the reference
screenshots and screen states. Each `fixture_id` used in `screen-inventory.csv`
is defined below; the 10 baseline fixtures are shared by the Light and Dark
reference images. Fixture definitions are reproducible state, not history.

## Baseline fixtures (with reference images)

### launcher-default (references: launcher-light/dark)

- Project hierarchy: none — launcher state before any project is opened.
- Project: two recent-project entries are shown (name + directory path +
  last-opened date/time) matching `PM / Screen / launcher-recent`.
- Window: 1440×900 logical pixels, scale 1.0, shared platform.
- Focus/selection: none; first-launch state has no recent entries
  (`PM / Screen / launcher-first-launch`).

### editor-single-default (references: editor-single-light/dark)

- Project hierarchy: Manuscript → Chapter One (editable, focused); Research has
  notes. Explorer open, Inspector open, status bar visible.
- Active pane: primary single pane; focus context primary. One tab (Chapter
  One).
- Document content: representative prose; first block is the Document Title.
- Editor: single default view; sidebar/Inspector uncollapsed; no local search.
- Window: 1440×900, scale 1.0, shared platform, reduced-motion off.

### editor-dual-default (references: editor-dual-light/dark)

- Project hierarchy: Manuscript → Chapter One (primary) and Chapter Two
  (companion).
- Pane/tab state: two panes; each pane shows a Manuscript document
  (`editor-dual-two-manuscript`); each tab strip is present.
- Focus: companion pane is focused on `...-right-focus`; toolbar targets the
  focused view (TOOL-003).
- Independent state: distinct cursor/scroll per view; same-document-two-views
  variant (`PM / Screen / editor-same-document-two-views`) shows one document in
  both panes with independent local search.
- Window: 1440×900, scale 1.0, shared platform.

### cards-default (references: cards-light/dark)

- Project hierarchy: Manuscript and Research roots with groups expanded per
  `cards-manuscript-default`; synopsis density.
- Cards: document cards with title + synopsis; group rows with disclosure.
- Selection: Manuscript root selected in `cards-research-selected` variant
  (Research selected).
- Window: 1440×900, scale 1.0, shared platform, reduced-motion off.

### global-search-default (references: global-search-light/dark)

- Project: multi-document project with known terms; search index built.
- Search state: query entered in `search-query-entry`; `search-streaming-results`
  and `search-result-navigation` show result groups; `search-no-results` for an
  unmatched query; `search-stale-deleted-results` includes a result whose
  source document was deleted.
- Workspace: Global Search replaced Explorer in the left sidebar; no scope
  selector.
- Window: 1440×900, scale 1.0, shared platform.

### history-default (references: history-light/dark)

- Project: session/date-grouped checkpoints per `history-session-date-grouped`;
  one named snapshot per `history-named-snapshot`; restore comparison per
  `history-restore-checkpoint`.
- Restore target: complete project checkpoint; changed lines highlighted at
  word level in the comparison pane.
- Window: 1440×900, scale 1.0, shared platform.

### settings-appearance-default (references: settings-appearance-light/dark)

- Project settings open on Appearance; current choice System
  (`settings-appearance-system`), with explicit Dark (`...-dark-override`) and
  Light (`...-light-override`) states.
- Window: 1440×900, scale 1.0, shared platform; OS appearance at the time of
  capture is not a fixture input (System is resolved at runtime).

### export-default (references: export-light/dark)

- Project: 4-document Manuscript; Entire Manuscript is the fixed export scope.
- Export state: `export-project-output-controls` with output path/name,
  title/page-break controls, numbering; `export-progress` nonblocking progress;
  `export-success` completion; `export-failure` error; `export-numbering`
  numbering review.
- Window: 1440×900, scale 1.0, shared platform; polite progress announcements.

### error-recovery-default (references: error-recovery-light/dark)

- Project: project crashed during editing; recovery log present;
  `recovered-after-crash` restores the last autosave checkpoint; editor focused
  after accept; recovery log remains disposable after durable save.
- Window: 1440×900, scale 1.0, shared platform.

### recently-deleted-default (references: recently-deleted-light/dark)

- Project: several documents recently deleted; `recently-deleted` lists them
  with the shared trash icon; `recently-deleted-fallback-location` shows the
  confirmable fallback destination when the original parent is unavailable.
- Window: 1440×900, scale 1.0, shared platform.

## Design-only screens (no reference image)

Screens without a baseline reference image share the fixture attributes above
by workspace family (editor, cards, search, comments, history, settings,
export, error) with the specific state noted in the `state` column of
`screen-inventory.csv` (e.g., `editor-local-find`, `cards-density-compact`).
Native rendering is intentionally variable for: window chrome, native menus,
native file dialogs, and OS spellcheck surfaces (see
`cross-platform-variants.md`).
