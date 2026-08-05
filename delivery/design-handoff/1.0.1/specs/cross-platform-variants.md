# Cross-Platform Variants

Records only intentional Windows, macOS, and Linux differences present in the
design. ParchMint implements all three platforms from the beginning (PLAT-001,
PLAT-002); shared surface is not duplicated per platform.

## Reference boards

- `PM / Reference / Platform / Windows · linked`, `... / macOS · linked`,
  `... / Linux · linked` (Page 13, all 1440×900) — window chrome, native menus,
  and platform-native conventions.
- `PM / Reference / Layout / layout-1280x720 · linked` (1280×720),
  `layout-1440x900 · linked` (1440×900), `layout-1920x1080 · linked` (1920×1080),
  `layout-2560x1440 · linked` (2560×1440) — the layout behavior across the
  supported window sizes; the minimum application-window size is 1280×720
  logical pixels and the app prevents resizing below it (WS-011).

## Intentional differences

| # | Difference | Requirement | Screen/component IDs | Notes |
|---|---|---|---|---|
| 1 | Native application menus and accelerator/shortcut assignment | PLAT-001/004 | Window chrome; workspace menus | Shortcuts are OS-idiomatic (Cmd vs Ctrl); no shortcut is authored into the prototype |
| 2 | Native open/save dialog (file pickers, directory choosers) | PLAT-001/002, PRJ-003 | create-project-dialog, export-project-output-controls | `PM / Reference / open-project-native-dialog-handoff` (Page 03) documents the native handoff surface; export dialog initial focus documented |
| 3 | Window chrome: title bar, traffic lights vs close/maximize positions | PLAT-001 | Workspace window | Reference boards record each OS layout; resizing below 1280×720 is prevented (WS-011) |
| 4 | Font rendering/family availability | PLAT-001, brief typography | All UI text | UI family Source Sans 3 with fallback stacks per `font-inventory.csv`; generic monospace for code-path presentation |
| 5 | Native copy/paste, dictionary, and spelling services remain OS-provided surfaces where applicable | PLAT-001 | settings-dictionaries, spellcheck surfaces | Per-document language override is out of v1 (project-default language only) |
| 6 | Modifier-key conventions for additive selection (Shift range; Cmd/Ctrl additive) | TREE-007 | explorer, cards-drag-multiselect | Platform adapter maps the additive-selection modifier |
| 7 | System appearance detection for `System` appearance choice | APPR-002 | settings-appearance-system | OS appearance changes follow immediately while running |

## Shared behavior (not re-specified per platform)

- All screens/boards not listed above are shared across platforms (`platform:
  shared` in `screen-inventory.csv`).
- Keyboard/focus order, F6 region cycling, and tab semantics are the same on
  every platform (`keyboard-focus-map.md`); only native menu/shortcut assignment
  and modifier conventions vary.
- Export output and canonical formats are platform-independent
  (architecture: deterministic canonical representations).