# Design Change Reconciliation

## Goal

Safely consume a new approved Penpot handoff after implementation has begun.

## Tasks

1. Validate the new immutable handoff and checksums.
2. Compare old/new manifests, tokens, assets, components, screens, states, interactions, accessibility annotations, and snapshots.
3. Map changes to product requirements, implementation components, visual baselines, and current feature branches.
4. Classify each change as compatible implementation detail, implementation task, product conflict, architecture impact, or deferred feature.
5. Produce:
   - design-diff report;
   - updated implementation map;
   - impact/task graph;
   - updated visual-regression plan;
   - open issues;
   - pending approval file.
6. Do not modify production code before approval.

## Stop

Stop for approval when the new handoff changes approved behavior or visual baselines. After approval, the Orchestrator Agent dispatches generated tasks through the normal feature pipeline.
