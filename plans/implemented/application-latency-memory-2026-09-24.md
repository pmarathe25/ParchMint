# Application latency and memory pass — 2026-09-24

This pass addressed work outside the Overview as well as work that was charged
to editor interactions when a large project was open. The changes keep active
UI state bounded and avoid repeated decoding and layout construction.

## Changes and reasons

- The Explorer now builds widgets for the visible rows plus overscan. It keeps
  fixed-height spacers for the rest of the expanded tree and refreshes the mount
  only when scrolling crosses a window boundary. Rename temporarily mounts all
  rows so its input can be found and focused; the row geometry stays the same.
- The History timeline caches filtered checkpoint indices, session headings,
  and row offsets. A scroll window starts with a binary search instead of
  rescanning and measuring every earlier checkpoint.
- Overview synopsis editors are created when editing begins and released when
  editing ends. Metadata editors are kept for the selected or active card,
  rather than every card visited during the session. Drafts still survive
  authoritative snapshot reconciliation.
- Project snapshot queries reuse a loaded document's word count while its
  revision is unchanged. A revision change recomputes the count; unloaded and
  removed documents leave the cache.
- The editor resolves both ends of a same-paragraph scalar range in one UTF-8
  scan. This removes a repeated prefix scan from common typing and selection
  paths without changing Unicode boundary behavior.

## Measurements

Release headless driver runs used the same test binary settings and machine,
three runs per version, with mode order reversed between repetitions. Values
below are medians of each run's p95 action time. The 200-document case has
80 words per document; the 10-document case has 5,000 words per document.
These are Iced software-renderer action times, not native presentation latency.

| Workload | Before | After |
| --- | ---: | ---: |
| 200 documents: one-character typing | 4.89 ms | 1.79 ms |
| 200 documents: typing after paragraph split | 5.12 ms | 2.06 ms |
| 200 documents: editor scroll | 6.01 ms | 2.46 ms |
| 200 documents: selection | 4.99 ms | 2.07 ms |
| 200 documents: RSS after typing | 40.1 MiB | 38.9 MiB |
| 10 long documents: one-character typing | 1.36 ms | 1.48 ms |
| 10 long documents: editor scroll | 1.68 ms | 1.68 ms |

The older benchmark clicked offscreen Explorer chapters during its later tab
switching phase. The new benchmark limits that phase to visible chapters so
windowing can retain its intended contract. Switching times and memory after
visiting all tabs are therefore not comparable between versions; the results
above are measured before that phase. Raw results are in
`/tmp/parchmint-app-performance-{cards,chapters}-2026-09-24.json` on the
measurement workstation.

In a debug same-binary History comparison with 50,000 checkpoints, 120 warm
timeline windows took 0.420 ms with the index versus 1,479.150 ms with the
legacy scan. Explorer widget construction for 100 expanded documents fell from
291.5 ms to 68.7 ms; with the branch collapsed, from 93.1 ms to 18.5 ms.
These are isolated construction measurements, not frame-time claims.

The real Wayland application was reviewed with Vulkan on a disposable copy of
`Manual-Test-Project-Ready.parchmint` and a generated 200-document project.
Nested Overview groups, Explorer scrolling, opening a scrolled chapter, and
the scrolled-row Rename action were exercised. Rename kept the selected row at
the same vertical position. The headless simulator also checks that geometry.

Focused editor-core and desktop cache tests, the complete UI-iced library
suite, and the UI-driver suite except for two pre-existing failures passed.
Those two failures also reproduce at `8c005ec`: the focus-mode test requests
the compact Format menu at wide width, and the triple-click test does not
persist italic formatting. The History breadcrumb and notification-label test
assertions were updated to match the rendered UI. Clippy and formatting checks
passed for the changed crates.

## Remaining cost

The snapshot contract still materializes document body `String`s for loaded
documents. Revision-keyed word counts remove one repeated decode, but not
those body copies. Eliminating them would require changing the snapshot and
consumer contracts to share immutable content or request only the active
document's body. The Explorer still traverses its expanded hierarchy to count
spacer rows on a view rebuild; it avoids constructing offscreen widgets, which
was the larger measured cost. These are candidates for a later contract-level
pass if profiles show them dominating again.
