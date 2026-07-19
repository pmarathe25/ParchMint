# Stage 05: research and split workspace

Difficulty: medium  
Recommended model: GPT-5.6 Terra, medium or high reasoning  
Depends on: stage 4  
Master references: `PLAN.md` “Product contract,” “User-experience contract,” and “Data-integrity, security, and privacy contract”

## Outcome

Add research notes and safe attachment handling, then make every document/reference view usable in two symmetric panes. A writer must be able to pin research while moving among manuscript sections, and that workspace must restore safely.

## Primary ownership

- Research and attachment use cases in `crates/parchmint-app`
- Attachment storage adapters in `crates/parchmint-storage`
- Research/split-pane bridge view models
- Split container, pane host, research, and preview QML/components
- Workspace-state codec and tests

## Required work

### 1. Research model and workflows

- Surface research groups, notes, and attachment references using stage 2 node types.
- Support create, import, rename, move, duplicate, trash, restore, tags, labels, and synopsis/notes metadata.
- Exclude research from manuscript compile by default and make any override explicit.
- Reuse the stage 3 editor for Markdown research notes without forking its behavior.

### 2. Attachment safety

- Copy imported attachments into `assets/` with UUID-based safe names while preserving a display name in metadata.
- Deduplicate only when content hashing is proven safe and does not surprise users.
- Reject traversal, symlink escape, unsupported size, and malformed references.
- Never execute or embed active attachment content.
- Provide native/safe previews for images, PDFs where supported, and plain text.
- Require an explicit system-open action for other file types and display the target application/path context.

### 3. Symmetric pane model

- Build two instances of a common pane host rather than separate “main” and “reference” implementations.
- Each pane can show an editor, attachment preview, outline, or card view.
- Support split horizontal/vertical, open in other pane, swap, close, focus next pane, and pin/unpin.
- Route edit, formatting, navigation, and search commands according to focused pane.
- Ensure closing or replacing one pane does not destroy the other pane's editor undo state.

### 4. Workspace persistence

- Persist selected nodes, pane contents, pin state, split orientation/ratio, open view type, cursor/scroll restoration hints, inspector visibility, and window state under `.parchmint/workspace.toml`.
- Use stable IDs and tolerate missing, moved, trashed, or externally deleted nodes.
- Version workspace state independently from the canonical project schema.
- If restore fails, open a safe default workspace and preserve diagnostic information without blocking the project.

## Acceptance gate

- A manuscript document remains editable while a research note or supported attachment is pinned in the other pane.
- Navigation changes only the unpinned pane and commands operate on the focused pane.
- Restart restores a valid workspace closely enough without making project opening depend on it.
- Malformed or stale workspace data falls back safely.
- Research is excluded from compile by default.
- Attachment import cannot escape the project, execute content, or overwrite an existing asset.
- Two large open documents remain within editor and memory budgets.

## Out of scope

- OCR, annotation, or editing of binary attachments
- More than two simultaneous panes
- Cloud research capture
- Global full-text search implementation

## Handoff

Create `docs/handoffs/05-research-and-split-workspace.md`. Include pane focus/command rules, workspace schema, supported preview types per platform, attachment threat model, and memory measurements with two documents open.
