# Workspace shell

The shell is Explorer on the left, a working surface in the middle, and
Inspector on the right. It begins below the 52 px top ribbon and ends above the
32 px status bar. The ribbon contains Editor, Cards, History, Recently Deleted,
Export, and Settings as a single-choice control. Its current destination uses a
restrained mint indicator without a hard outline. Global Search opens from the
Explorer header and replaces Explorer in the left sidebar; it is neither a
ribbon destination nor a scoped search.

Sidebars and splitters resize between the minimum and maximum widths defined by
design tokens. Sections, fields, rows, and headers stretch to the pane width;
controls at an edge stay anchored there. The central workspace uses all
remaining width and height while the reading column stays bounded. Its panes,
tabs, toolbar, and canvas resize with the window. The minimum window is 1280 ×
720 logical pixels. Do not resize below it or substitute a mobile,
automatically collapsed, or feature-reduced layout. At 1280 × 720, 1440 × 900,
1920 × 1080, and 2560 × 1440, the layout must not clip controls.

The formatting toolbar spans editor panes only. Never outline a focused editor
pane; show focus through its tab and visible focus state. Icon-only controls use
familiar metaphors, clear labels or tooltips, symmetric icon padding, and no
reserved label space. Use the same trash icon for Recently Deleted and
destructive row actions.

Use explicit text, multiline, select, checkbox, or radio controls for editable
values. Placeholder text is grey and italic. Read-only information is visibly
distinct in both appearances. Multiline fields wrap their text and have an
intentional rule for growing or scrolling.

The status bar places the Explorer visibility control at the left and the
Inspector visibility control at the far right. Each uses the selected mint
treatment while its pane is shown. The contextual document-History control also
belongs in the status bar.

Focus, selection, disabled, warning, error, comment, search-match, and save
states must meet contrast requirements without color alone. Reference boards
may show roles, hierarchy levels, focus order, and keyboard actions for future
work, but they do not add a v1 assistive-technology requirement.

## Keyboard and focus

In Editor mode, **F6** cycles major regions in this exact order:

```text
mode switch -> formatting toolbar -> Explorer -> active tab -> focused editor
-> Inspector -> status bar
```

Tab/Shift+Tab and arrow navigation follow platform convention within each
region. Initial focus is:

| Context | Initial focus |
| --- | --- |
| Project opens | Focused editor, for authoring |
| Create Project dialog | Project name |
| Export | Summary or error summary |
| Save-failure dialog | Safe action |
| Restore confirmation | Checkpoint name and whole-project impact |
| Recently Deleted | Item list |

A dialog traps focus. Escape or an explicit close returns focus to its invoker.
If that control no longer exists, return focus to the closest surviving related
control or its containing region. Menus and local context menus return focus to
their invoker, including the focused editor after its context menu closes.

Each dialog has a clear title and an associated description where needed. A tab
shows its full document title through its tooltip when the visual title is
truncated; its close control identifies that document.

Focused, selected, disabled, warning, error, comment, search-match, and save
states use semantic focus rings and a non-color cue in both appearances. The
tab strip and visible focus state, never an editor-canvas outline, communicate
focused editor-pane state.
