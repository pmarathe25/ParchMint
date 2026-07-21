# Plans

This directory contains temporary, multi-step work artifacts. Maintained
documentation never lists which plans are active. Delete completed plans after
durable results move into code, tests, guides, format contracts, release
records, or ADRs.

## Naming and format

Use `NN-difficulty-kebab-case.md`, continuing the sequence visible in Git
history. Difficulty is `easy`, `medium`, `hard`, or `very-hard`.

Plans use the initial implementation format:

```markdown
# Stage NN: outcome

Difficulty: level

Recommended model: model/capability and reasoning level

Escalate when: concrete condition requiring stronger reasoning or owner input

Depends on: prior stages, decisions, or external prerequisites

Master references: maintained contracts and source-of-truth documents

## Outcome
## Primary ownership
## Required work
### 1. Ordered workstream
## Verification
## Acceptance gate
## Out of scope
## Handoff
```

The outcome states the user-visible or engineering result. Required work is
ordered and testable. Verification names the evidence to collect; the
acceptance gate defines completion. Out of scope prevents accidental expansion.

`Handoff` records what a subsequent owner needs: protocols, evidence, platform
gaps, risks, and deliberate deferrals. Put that evidence in the plan closure,
release record, code, tests, or maintained docs. Do not create a standalone
handoff file unless the repository owner explicitly requests one.

Delete a completed plan instead of preserving it as a second changelog. Use an
[ADR](../docs/architecture/adr/) for durable cross-cutting decisions.
