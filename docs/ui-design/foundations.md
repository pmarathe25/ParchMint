# UI foundations

ParchMint is a calm, editorial desktop application for sustained writing. Keep
authored prose prominent and chrome quiet.

- Use compact desktop density, flat surfaces, restrained tonal state layers,
  minimal elevation, and four-pixel default corners.
- Use charcoal neutrals with a restrained mint accent. Material Design informs
  interaction states and icon metaphors, not the application's visual identity.
- Keep application UI typography separate from project-controlled prose styles.
- Make editable, selected, focused, disabled, warning, error, and read-only
  states recognizable without color or hover alone.
- Use spacing, hierarchy, selection, and familiar icons to make common actions
  clear. Keep text for authored content, field labels, counts, unfamiliar or
  destructive actions, menus, and confirmations.

## Type, icons, and assets

- **Application UI:** Source Sans 3 at weights 400, 500, 600, and 700. It must
  be available on Windows, macOS, and Linux through a bundled or build-time
  resolved font. The legacy `sourcesanspro` alias resolves to Source Sans 3.
- **Prose samples:** Source Serif 4 only. Project-authored prose styles remain
  project data and are never replaced by application appearance tokens. If
  unavailable, Source Serif 4 sample text falls back to Source Sans 3.
- **Legacy compatibility roles:** Inter, falling back to Source Sans 3; Inter
  is not the primary UI family.
- **Code and paths:** the available platform monospace stack:
  `ui-monospace`, Menlo, Consolas, or Liberation Mono.
- Fonts bundled with ParchMint or resolved into a build must be available under
  the SIL Open Font License 1.1 or another GPL-compatible license. Menlo and
  Consolas are unbundled platform fallbacks and remain covered by their
  operating-system licenses.
- Icons are archive-authored, monochrome, and Material-aligned. Semantic tokens
  color them at runtime. Use one consistent optical size and stroke across the
  icon family. Do not create theme-specific icon variants or add raster fills
  to production screens; export only product-used vectors.

Define separate UI body, compact body, label, heading, tab, menu, path/code, and
status styles. Each style has an explicit font size, weight, and line height in
the native design source.

## Appearance and tokens

The Penpot source provides a complete Light value set and Dark value set for
the same semantic tokens. `System` is the default appearance and resolves at
runtime from the operating system.
Changing appearance updates every open window without restart and never enters
project undo, save, history, authored styles, or export output.

Dark uses fully dark application, sidebar, Inspector, toolbar, editor-chrome, and
manuscript surfaces; do not leave a light prose sheet inside dark chrome. Light
and Dark share structure, variants, and interaction states.

Tokens cover:

- application, sidebar, Inspector, manuscript, elevated, menu, dialog,
  read-only, and overlay surfaces;
- primary, secondary, disabled, inverse, path/code, placeholder, and link
  text; borders, separators, splitters, focus rings, and scrims;
- accent default, hover, pressed, and selected states; focused and unfocused tab
  and pane states; search match and active search match; comment anchor and
  comment status; dirty, saving, saved, and error save states; warning, error,
  destructive, success, and focused and unfocused selection states; and
- spellcheck underlines and menus.

Name tokens for purpose, not color. Production components bind tokens instead
of hard-coding theme-dependent values.

Use a compact spacing scale and symmetric control padding. The workspace top bar
is 52 px high, the status bar is 32 px high, and 20 px core icons sit in 32–36
px controls. The native design source defines explicit toolbar, tab, tree-row,
menu-row, and card dimensions. It also defines minimum pointer targets,
focus-ring offsets, pane and splitter limits, and four-pixel default radii with
deliberate menu and dialog exceptions.

Define visible effects for focus, selection, pressed controls, menus, dialogs,
tooltips, and errors. Avoid nonessential animation. Reduced-motion preference
integration is future work.
