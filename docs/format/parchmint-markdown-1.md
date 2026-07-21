# ParchMint Markdown 1.0 grammar

> Read when changing Markdown parsing, semantic editing, serialization,
> diagnostics, or exporter interpretation.

Documents are UTF-8 CommonMark with selected GFM tables, task lists,
strikethrough, and footnotes. A YAML mapping delimited by `---` at byte zero
contains metadata. Unknown keys are retained.

Stable style references use UUID-like immutable IDs, never display names:

```markdown
[character text]{.parchmint-style style-id="018f0be2-a8ea-7d2d-89ea-45aa663708d5"}

Paragraph text. {#optional-anchor .parchmint-style style-id="018f0be2-a8ea-7d2d-89ea-45aa663708d4"}
```

Alignment wraps one or more paragraphs in a fenced div:

```markdown
::: {.parchmint-align align="center"}
Centered text.
:::
```

Allowed `align` values are `left`, `center`, `right`, and `justify`.
Superscript and subscript use `<sup>` and `<sub>`. A page break is exactly:

```markdown
<!-- parchmint:page-break -->
```

Project attachments use `asset:<stable-asset-id>` destinations. Display names
are alt text, not identity.

Unsupported block source is retained as an opaque semantic node. The initial
explicit fixture spelling is:

````markdown
```{=parchmint-opaque source-format="future-markdown"}
source retained exactly
```
````

The codec may also classify unrecognized extension blocks as opaque without
rewriting them into that spelling. Opaque nodes are protected and visibly
identified in WYSIWYG mode. Attribute output order is `id`, class names,
`style-id`, then remaining keys in lexical order.

## Supported semantic matrix

| Construct | Canonical spelling | Editing behavior |
|---|---|---|
| Paragraph | CommonMark paragraph | Semantic inline runs and paragraph attributes |
| Title/headings | ATX `#` through `######` | Level is explicit, never inferred from font size |
| Emphasis | `*italic*`, `**bold**`, `~~strike~~` | Character format |
| Super/subscript | `<sup>…</sup>`, `<sub>…</sub>` | Character vertical alignment |
| Links | `[label](destination "title")` | `http`, `https`, `mailto`, relative, and `asset` destinations |
| Images | `![alt](asset:<uuid> "title")` | Project asset identity is the destination |
| Lists/tasks | CommonMark markers and GFM tasks | List boundaries, ordered starts, nesting, continuations, and checked state are semantic |
| Block quotes | CommonMark `>` | Source-aware supported block |
| Code | Indented or fenced code | Info string and text are retained |
| Tables | GFM pipe table | Source-aware supported block |
| Footnotes | GFM definition/reference | Source-aware supported construct |
| Thematic/scene break | CommonMark thematic break | Semantic thematic break |
| Alignment | `::: {.parchmint-align align="…"}` | `left`, `center`, `right`, or `justify` |
| Named styles | `.parchmint-style style-id="…"` | Stable paragraph or character identity |
| Page break | `<!-- parchmint:page-break -->` | Protected visible compile marker |

Untouched supported blocks retain their exact UTF-8 source slice. A block that
is semantically edited is reconstructed deterministically with two trailing
newlines, ATX headings, `-` unordered markers, sequential `.` ordered markers,
collision-resistant backtick fences, and the attribute ordering above. The
inline codec is escape-aware: it decodes only the backslash escapes emitted by
the serializer, leaves percent escapes in destinations unchanged, supports
escaped link brackets/parentheses and quoted titles, and reaches a semantic
fixed point after one serialization. Reference links remain source-backed
opaque content until a lossless reference-definition representation exists.
Malformed attribute quotes are not partially consumed.

## Diagnostics and unsupported input

- Source spans are absolute UTF-8 byte ranges, including the front-matter
  offset. Diagnostics have a stable machine code, severity, range, and message.
- Duplicate anchor IDs are warnings; their source is retained. Duplicate
  top-level YAML keys are warnings and the last YAML value is semantic.
- `.parchmint-style` without `style-id`, and IDs absent from the project style
  catalog, are visible warnings. Source and direct appearance survive.
- An unknown fenced div, explicit opaque fence, unsupported HTML block, or
  paragraph containing unsupported inline HTML is a source-backed opaque block.
- Unclosed front matter is a hard parse error. Unclosed fences/divs are opaque
  nodes with diagnostics so raw mode retains the complete buffer.
- Returning from raw mode is forbidden after a hard parse error until the user
  fixes the buffer or explicitly discards it.
- The default hostile-input limits are 16 MiB source bytes, 100,000 blocks,
  inline/fenced-div depth 64, 1,000,000 delimiter inspections, and 1,024
  retained diagnostics. Crossing a limit returns a typed resource-limit error;
  it never recurses without a bounded depth.
