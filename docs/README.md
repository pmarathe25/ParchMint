# ParchMint documentation

ParchMint's documentation explains how to use the application and how the
implementation fits together. Automated tests define supported behavior and
protect it from regressions.

## Use ParchMint

The [user guide](user-guide.md) covers the launcher, project organization,
writing, comments, search, History, recovery, settings, and export.

## Understand the implementation

- [Architecture](architecture/architecture.md) explains the application flow,
  crate boundaries, stored data, and ownership model.
- [UI design](ui-design/README.md) describes the visual system, workspace
  layout, interaction patterns, and maintained Penpot source.
- Each crate has a `README.md` beside its `Cargo.toml` with its public contract
  and implementation details.

Development and test commands are listed in the repository [README](../README.md).
