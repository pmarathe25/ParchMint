# Editor and tabs

- Exactly one always-visible formatting toolbar targets the focused editor view:
  style select, visually styled B/I/U/S glyphs, split list, block quote, link,
  Scene Break, Page Break, and a labeled Comment action. Comment opens the
  same anchored draft as the editor context menu.
- B/I/U/S show the focused selection's marks or pending caret format. The
  toolbar scrolls horizontally when its controls do not fit the available width.
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
- Wheel scrolling through a companion Research note preserves the current
  editing target. Pointer selection or keyboard focus changes that target.
- A newly selected tab shows a loading state until its document is mounted.
  Text from the previous tab must not appear under the new document title.

## Dual-pane editor states

Keep these states distinct:

| State | Design rule |
| --- | --- |
| Two Manuscript documents | Each pane displays a different Manuscript document. |
| Same document, two views | Both panes display one document. Body content, formatting, comments, undo history, save state, and word count are shared; cursor, selection, scroll, viewport, focus, and local search remain independent per view. |

Neither state substitutes for the other.
