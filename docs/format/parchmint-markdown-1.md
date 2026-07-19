# ParchMint Markdown 1.0 draft grammar

Status: syntax accepted by ADR-0005; complete codec implementation and schema
freeze belong to Stages 02–03.

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
`style-id`, then remaining keys in lexical order. Stage 03 owns comprehensive
normalization rules and diagnostics.
