#!/usr/bin/env python3
"""Rebuild small formatting symbols with shared 24px geometry and stroke weight.

The imported pencil contours remain appropriate for navigation artwork, but lose
legibility at 20px for lists and text formatting. These optical-size variants
retain the shared brown tint without layered tracing noise.
"""
from pathlib import Path

OUT = Path(__file__).resolve().parents[1] / 'crates/parchmint-ui-iced/assets/pencil'
paths = {
    'bold': 'M7 5h6a3.5 3.5 0 0 1 0 7H7m6 0a3.5 3.5 0 0 1 0 7H7V5',
    'italic': 'M10 5h8M6 19h8M14 5l-4 14',
    'underline': 'M7 5v7a5 5 0 0 0 10 0V5M5 20h14',
    'strikethrough': 'M17 7c-1-3-10-3-10 1 0 2 3 3 5 4m-5 5c1 3 10 3 10-1 0-1-1-2-2-2M4 12h16',
    'align-left': 'M4 5h16M4 10h10M4 15h16M4 20h10',
    'align-center': 'M4 5h16M7 10h10M4 15h16M7 20h10',
    'align-right': 'M4 5h16M10 10h10M4 15h16M10 20h10',
    'align-justify': 'M4 5h16M4 10h16M4 15h16M4 20h16',
    'bulleted-list': 'M9 5h11M9 12h11M9 19h11',
    'numbered-list': 'M10 5h10M10 12h10M10 19h10',
    'line-spacing': 'M4 4v16M2 6l2-2 2 2M2 18l2 2 2-2M10 5h10M10 12h10M10 19h10',
    'block-quote': 'M5 6v12M10 7h10M10 12h10M10 17h6',
}
for name, d in paths.items():
    extras = ''
    if name == 'bulleted-list':
        extras = ''.join(f'<circle cx="4" cy="{y}" r="1.5" fill="#80664f"/>' for y in (5, 12, 19))
    elif name == 'numbered-list':
        extras = '<path d="M3 3l1-1v5M3 7h2M2.5 11c0-2 3-2 3 0 0 1-3 2-3 3h3M2.5 18c1-1 3-1 3 .5 0 .7-.7 1-1.5 1 .8 0 1.5.3 1.5 1 0 1.5-2 1.5-3 .5" fill="none" stroke="#80664f" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>'
    (OUT / f'{name}.svg').write_text(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="{d}" fill="none" stroke="#80664f" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/>{extras}</svg>\n')
