# Writing workflow improvement plan

ParchMint's next development work prioritizes dependable Linux authoring and
clear editing targets. Research remains a first-class workspace: authors must
be able to consult and edit character notes, background material, and lore
beside the manuscript.

## Representative workflow

Use a novel with a few groups, several chapter-length documents, and a Research
collection. Exercise repeated switches between chapters while a Research note
stays open. Include selection, formatting, comments, undo, structural changes,
save, History, close, and reopen. Keep the 250,000-word single-document fixture
as a stress case; it is not the typical project structure.

Penpot provides visual inspiration. Interaction changes and clearer layouts
can supersede it; reference images should only change after reviewing the new
design in the application.

## Implemented: editing targets and retained changes

The initial source review identified these concrete paths:

- Local editor commands went through an asynchronous task and resolved their
  document and selection when the task completed. Native authoring commands
  now execute within the input event.
- Canvas messages identified a reusable view without its mount generation.
  They now carry that generation, as do view-specific asynchronous editor
  effects. A tab awaiting its document displays a loading state.
- Comment submission used the focused pane and current selection. Drafts now
  retain their original target and selection. An invalidated target leaves the
  draft available for reanchoring. Reply and edit drafts clear after success.
- A presentation failure could follow an accepted edit without scheduling its
  revision for persistence. Both mounted input and toolbar command error paths
  now check for an accepted revision and retain it for save and recovery.
- Scrolling a Research pane could change the formatting target. Wheel scrolling
  now preserves the editor focus target.
- The usual desktop build omitted the documented diagnostics. Diagnostics are
  now enabled by default, with a bounded in-memory event capture as well as the
  bounded log file.

The toolbar also exposes a labeled Comment action. Regression coverage includes
delayed input, comment ownership across panes, retained drafts, a layout failure
after formatting, and a manuscript/Research flow through save and reopen. The
combined flow checks that an unchanged explicit save adds no checkpoint.

## Implemented: persistence and History

Document loads merge matching revisions into the current snapshot without
reconciling stale outline state over live changes. Mounts and search navigation
check that their document is still active. Retained hosts release keyboard
focus across document sessions; a regression reproduces and prevents duplicate
typing when creating Research notes.

History reads document paths from each checkpoint's manifest, supports legacy
path-derived document identities, and reports missing declared files as errors.
The Current comparison uses the mounted draft, including unsaved edits. Tests
inspect actual checkpoint HTML and comment bytes, then verify save and reopen.
The workspace suite also covers save failures and interrupted-session recovery.

## Implemented: performance

Undo stores changed paragraph spans, bounded to 256 operations and roughly
64 MiB per stack (retaining one oversized latest operation). Canonical
serialization runs on demand outside the adapter lock. Background UI work uses
four workers and a 128-job queue, with nonblocking overload errors.

Optimized Linux measurements for eight 20,000-word chapters plus 2,000 words of
Research: typing p95 4.49 ms, selection 0.17 ms, scrolling 0.23 ms, retained
chapter switching 3.50 ms, and projection 1.11 ms. Process RSS rose from 11,520
to 17,772 KiB after 512 edits. These editor-side measurements exclude file I/O
and final screen presentation. A 16 ms editor-work target is a useful local
budget, not a machine-independent test threshold. The optimized full-application
20,000-word test measured 336 ms to open, 175 ms median save (199 ms maximum),
and 84 ms to reopen, preserving all five edit markers. Both measurements remain
opt-in tests.

## Implemented: writing workspace

Explorer has visible New document and New group actions with their destination.
Naming a new Research document opens it beside the manuscript. Formatting
buttons show active marks and support typing at a collapsed caret. Word counts
update live and exclude Research from the manuscript total. The toolbar scrolls
when narrow; notifications have separate space from save status.

Headless checks now load the desktop's fonts and deliver viewport redraws.
Native capture waits for completed draws, fixing premature blank screenshots.
Native light and dark views were reviewed at 1280×800 and 1440×900; the Linux
compositor clamps larger 2× requests to the available display. Text clipping
also has a pixel regression at both scales. Research remains an independent
companion pane, with shared editing only when both panes open the same document.

Keep regressions in component tests or existing end-to-end flows. Use the
pinned toolchain and locked Cargo commands with one compilation job.
