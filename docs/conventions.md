# Documentation conventions

> Read when creating, moving, or substantially rewriting documentation.

Documentation should help a reader act without loading unrelated context.

## Put information in one place

| Information | Home |
|---|---|
| Product promises and exclusions | `docs/product/` |
| User tasks | `docs/user-guide/` |
| Current system design and ownership | `docs/architecture/` |
| Durable decision rationale | `docs/architecture/adr/` |
| Setup, coding, testing, localization | `docs/development/` |
| Normative schemas and behavior tables | `docs/format/`, `docs/reference/` |
| Release requirements and procedures | `docs/release/` |
| Privacy, security, licensing | `docs/legal/` |
| Temporary implementation work | `plans/`, never `docs/` |

Indexes route; they do not repeat their children. A detail belongs in the
narrowest authoritative page and is linked elsewhere.

## Write for selective loading

- Start with `> Read when: …` or an equally direct routing sentence.
- Lead with the decision, invariant, or action.
- Use short sections, bullets, tables, and examples only when they reduce scan time.
- Name code, commands, paths, and failure behavior precisely.
- Link to a deeper contract instead of summarizing it twice.
- Keep normative details even when they are not short; correctness beats brevity.

## Keep docs durable

Do not record active plans, current blockers, pending rows, dated test results,
temporary workarounds, agent/model assignments, or implementation-stage history
in maintained docs. Put work status in a plan, issue, change description, or
release evidence bundle.

Update docs when public behavior, commands, ownership, formats, or support
changes. Delete a page when its unique purpose disappears. Run the local-link
check described in [testing](development/testing.md) after moving files.
