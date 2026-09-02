# Editor and tabs

- Exactly one always-visible formatting toolbar targets the focused editor view:
  style select, visually styled B/I/U/S glyphs, split list, block quote, link,
  Scene Break, and Page Break. It has no Add Comment control.
- Every populated primary or companion pane has a tab strip, including a
  one-tab Research companion. Tabs are 32 px high with a fixed close region;
  long titles use the longest prefix that fits, followed by an ellipsis, without
  entering the close region. Overflow shrinks tabs uniformly to a minimum that
  still shows the first character, ellipsis, and close control. The tooltip
  contains the full title.
- Beneath each populated pane's tab strip, show its own compact muted path,
  such as `Manuscript › Part One › Chapter One`. The document path follows the
  pane's active tab, not Explorer selection. On narrow panes, ellipsize older
  ancestors before the document title; paths are context, not navigation.
- Only the focused pane's active tab is mint. An unfocused active tab uses a
  neutral selected treatment. Local Find is below tabs; Local Replace begins
  collapsed behind a selected-state toggle.

## Dual-pane editor states

Keep these states distinct:

| State | Design rule |
| --- | --- |
| Two Manuscript documents | Each pane displays a different Manuscript document. |
| Same document, two views | Both panes display one document. Body content, formatting, comments, undo history, save state, and word count are shared; cursor, selection, scroll, viewport, focus, and local search remain independent per view. |

Neither state substitutes for the other.
