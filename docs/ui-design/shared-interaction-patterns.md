# Shared interaction patterns

`PM/InspectorSection` is the shared disclosure pattern for Inspector sections,
Manuscript and Research roots, grouped Search results, and comparable Cards
groups. It has a disclosure row, necessary content, and a short bottom
separator only: no outer fill or outline, top separator, or final-item
separator. Center disclosure icons and leave enough room to prevent clipping or
overlap.

Context menus use shared surface, item, divider, elevation, and overlay-order
primitives. A menu defines only action order, labels, icons, and intentional
states. Use relevant icons, compact symmetric padding, the menu elevation token,
and an overlay layer that stays above the surrounding screen.
