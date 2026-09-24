# Visual language

**Purpose:** Document ParchMint's shared visual language for contributors.

The design language uses surface hierarchy, type, spacing, controls, and
interaction states to make the writing workspace quiet and predictable. Content
has the strongest contrast; navigation, settings, and review tools use the same
cues in light and dark appearances.

## Surfaces and separation

Use the semantic surfaces in `components::Surface`: application canvas, sidebar,
panel, manuscript, elevated content, dialog, and status bar. Keep adjoining flat
surfaces unboxed. A one-pixel line in `palette.divider` marks their shared edge:
the navigation rail has a right divider and the status bar has a top divider.
Resizable sidebars keep a wider pointer target around a one-pixel visible line.
Cards, fields, menus, and dialogs can have full outlines because they are distinct
objects. Use shadows only for dialogs and temporary overlays.

## Text and spacing

Use Source Sans 3 for interface text. The shared sizes are 14 px for body text,
12 px for supporting text and labels, 16 px for section headings, and 24 px for
page titles. Manuscript typography follows document styles. Use the shared
4, 8, 12, 16, 24, and 32 px spacing tokens; keep related controls close and give
separate groups more space. Compact controls are 28 px high and regular controls
are 36 px high.

Give non-interactive text a clear role. Use primary text for content, entered
values, and section headings; use secondary text for descriptions, field labels,
breadcrumbs, counts, timestamps, and placeholders. Use `components::page_title`
for page titles and `components::muted_label` for short supporting labels. Keep a
field's persistent label visible when its purpose would otherwise depend on a
placeholder. Empty states should explain the state in one short sentence, with
any available action rendered as a separate button. Do not give static text a
button background, hover state, pointer cursor, or disabled-control color. Pair
status color with a word or icon so meaning does not depend on color alone.
Use sentence case for labels in lists, forms, and navigation.

## Buttons and text actions

Start with `components::semantic_button` for a standard secondary text button.
Use `components::button_label` and `components::button_style` for other roles,
with the shared control heights and spacing tokens for padding. Choose the
button kind by its purpose:

| Kind | Appearance | Use |
| --- | --- | --- |
| Primary | Accent fill and contrasting text | The main commit or create action in a view or dialog; usually one per action area. |
| Secondary | Panel fill and a thin outline | Alternatives and Cancel beside a primary action. |
| Quiet | No resting fill; subtle hover and focus feedback | Inline actions, list rows, and compact controls. A text-only quiet action needs a clear verb and a reliable hit target. |
| Destructive | Destructive color and an explicit label | The final delete or discard action. |
| Tab | Selected-state fill | Navigation between peer views, never a form submission. |

Write action labels as short verbs or verb phrases in sentence case, such as
“Save”, “Add field”, and “Delete thread”. Keep Cancel before the primary action
in dialog footers. A clickable label is still a button: it needs hover, keyboard
focus, and disabled states. Reserve confirmation for an action that would actually
lose work. Show a configured keyboard shortcut in the tooltip for any action
that has one.

## Icons

Use the pencil SVG catalog through `icons::icon_sized`. Toolbar symbols share a
24 px source grid and render at 20 px with centered optical bounds. Navigation
symbols render at 24 px; small inline actions use 16–18 px. Match visual weight
and vertical alignment before adding an icon to a shared row. Prefer text when a
symbol needs an explanation to identify its action.

## Dragging and dropping

Use the shared hierarchy drag gesture for reorderable items: a short movement
threshold separates a click from a drag, a floating copy follows the pointer,
and a thin mint insertion line or outlined destination shows exactly where the
item will land. Keep the source and surrounding layout readable during the drag.
Release anywhere in the window commits the current valid target; Escape or
leaving the window cancels it. Do not require a precise release over a small
handle or start a drag on the initial press. Sections such as metadata visibility
must stay visible even when empty so each destination remains discoverable.

## Color and interaction

Choose colors by role from `ParchMintTheme::palette`, never by a screen-specific
RGB value. Use the accent for the current selection and primary action. Selected
items use the shared subtle accent fill. Focus uses the two-pixel mint focus ring;
errors use the same width with the error color. Disabled controls remain readable
without appearing active.

## Applying the language

Build controls with `components` and take dimensions and type from
`design_tokens`. Put workflow-specific detail in the screen, while shared states
stay in the components. Keep optional detail behind selection, expansion, or a
popover: collapsed Overview cards and groups show chosen fields, expanding a
group also reveals its other fields, and selected comments reveal their thread.
A comment hover gives a brief preview without opening editing controls. Settings
lists identify items without repeating their full configuration. Check the result
in light and dark appearances at normal and compact widths using the
[native UI review](../tests/parchmint-ui-driver/USABILITY.md).
