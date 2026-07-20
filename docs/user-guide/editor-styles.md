# Editor and styles

The editor supports headings, paragraphs, bold, italic, strike, super/subscript,
lists and tasks, links, images, alignment, thematic breaks, page breaks, and
named paragraph or character styles. Style references use stable IDs, not
display names. Page breaks are semantic compile markers, not page-accurate
layout.

Supported Markdown is source-aware and repeatedly saves deterministically.
Unknown front-matter keys and unsupported blocks remain visible opaque source;
they are not silently discarded. Rich paste is sanitized to the supported model;
plain-text paste remains literal text. Raw source mode must be fixed or
explicitly discarded before returning from a hard parse error.
