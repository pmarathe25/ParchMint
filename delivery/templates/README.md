# Delivery Template Usage

These temporary delivery files are immutable blueprints. Agents copy the relevant template to its runtime destination, fill it with current commits, checksums, IDs, evidence, and results, and commit the instantiated artifact. They do not record runtime state by editing the template itself, and they do not promote a template into maintained documentation.

## Design handoff

Used by the Design Agent when assembling `delivery/design-handoff/<version>/`:

| Template | Destination |
|---|---|
| `design-handoff/README.md` | `delivery/design-handoff/<version>/README.md` |
| `design-handoff/design-manifest.yaml` | `delivery/design-handoff/<version>/design-manifest.yaml` |
| `design-handoff/tokens-README.md` | `delivery/design-handoff/<version>/tokens/README.md` |
| Design-handoff CSV/Markdown spec templates | `delivery/design-handoff/<version>/specs/` |

`design-manifest.schema.json` stays under `delivery/templates/` and is the validation schema used by S00. Exported `.penpot`, token JSON, assets, reference images, and checksums come from Penpot or the handoff assembly process rather than a template.

## Design reconciliation

S10 copies all files under `delivery/templates/design-reconciliation/` to `delivery/design-reconciliation/<handoff-version>/`, fills the implementation mappings and open issues, and leaves `approval.yaml` pending for G10 product-owner approval.

## Pipeline

| Template | Owner | Destination |
|---|---|---|
| `pipeline/state.yaml` | Orchestrator | `delivery/state.yaml` |
| `pipeline/dispatch.yaml` | Orchestrator | `delivery/runs/<stage-id>/<run-id>/dispatch.yaml` |
| `pipeline/status.yaml` | Stage agent | `delivery/runs/<stage-id>/<run-id>/status.yaml` |
| `pipeline/handoff.yaml` | Stage agent | `delivery/runs/<stage-id>/<run-id>/handoff.yaml` |
| `pipeline/report.md` | Stage agent | `delivery/runs/<stage-id>/<run-id>/report.md` |
| `pipeline/approval.yaml` | Orchestrator/product owner | The applicable `delivery/gates/` or proposal approval path |
| `pipeline/task.yaml` | S100 | `delivery/generated-tasks/<wave-id>/<task-id>.yaml` |
| `pipeline/traceability.csv` | S00 | `delivery/traceability.csv` |

The run's instantiated `dispatch.yaml`, not the template, controls a stage agent's ownership and required evidence.

## Release

S130 copies the machine-readable files under `release/` to `delivery/release-evidence/<candidate-version>/`, adds the required evidence directories and package hashes, validates the complete package, and leaves `release-approval.yaml` pending for G90.
