# Stage 04: binder, outline, and cards

Difficulty: medium  
Recommended model: GPT-5.6 Terra, medium or high reasoning  
Depends on: stages 2–3  
Master references: `PLAN.md` “Product contract,” “User-experience contract,” “Domain and dependency contract,” and “Cross-stage quality gates”

## Outcome

Deliver the primary manuscript organization experience: project lifecycle UI, a scalable hierarchical binder, summary-first outline and card views, metadata inspector, and recoverable structural operations. A user should be able to outline and reorganize an entire novel before writing any body text.

## Primary ownership

- Binder/outline/card models in `crates/parchmint-bridge`
- Structural and metadata use cases in `crates/parchmint-app`
- Main-window, binder, outline, card, and inspector QML
- Structural UI and 10,000-node performance tests

## Required work

### 1. Project shell

- Implement create/open/close/recent-project flows and actionable validation errors.
- Complete the main-window layout with collapsible binder and inspector, central view switching, menus, command registry, and focus routing.
- Persist noncritical window geometry and view preferences without mixing them into canonical project data.

### 2. Binder

- Implement a lazy hierarchical Qt model backed by Rust snapshots/events.
- Support create, rename, duplicate, multi-select, reorder, reparent, indent, outdent, move up/down, trash, and restore.
- Distinguish drop-before, drop-after, and drop-inside with clear indicators.
- Provide keyboard equivalents for all drag operations and context/menu commands.
- Restore expansion and selection state when possible without making workspace state required for project validity.
- Prevent invalid cycles and communicate domain validation failures without leaving optimistic UI residue.

### 3. Summary outline

- Display hierarchical rows with title, synopsis, status, label, word count placeholder/current count, and configurable columns.
- Allow direct title, synopsis, status, and label editing through domain commands.
- Filter while retaining ancestor context and stable selection.
- Make sorting a view operation unless the user explicitly commits a reordered binder structure.
- Support subtree focus and a breadcrumb back to the full project.

### 4. Cards

- Implement virtualized compact cards showing title, synopsis, status, and label.
- Keep selection and structural operations consistent with binder/outline.
- Support density changes and ordered drag/drop, but not freeform spatial placement.

### 5. Inspector and integration

- Implement synopsis, metadata, tags, include-in-compile, notes placeholder, and basic counts.
- Open the selected manuscript document in the stage 3 editor.
- Define predictable selection behavior for multi-selection and group nodes.
- Keep binder, outline, cards, inspector, and editor synchronized only through Rust domain events/view models.

## Acceptance gate

- A user can create Part/Chapter/Scene hierarchies, populate only synopses, and review the entire outline without opening bodies.
- All structural and metadata changes survive restart and are undoable where the master plan requires it.
- Trash retains canonical documents until explicitly emptied.
- Binder, outline, and card selection/order remain consistent after filtering, undo, external metadata changes, and restart.
- Initial display, scrolling, expansion, selection, and mutation meet the 10,000-node budgets.
- Every drag operation has a tested keyboard equivalent and accessible action name.

## Out of scope

- Research attachment workflows
- Second/split pane
- Full-text search and final counts
- Compile preset UI
- Final command palette and accessibility certification

## Handoff

Create `docs/handoffs/04-binder-outline-and-cards.md`. Document tree-model update semantics, selection rules, structural command mappings, virtualization measurements, and stable QML components stage 5 may reuse.
