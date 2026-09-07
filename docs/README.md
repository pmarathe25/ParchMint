# ParchMint documentation

ParchMint stores projects in ordinary files and connects a native writing UI
to separate editing, persistence, History, search, and export components.
Tests are the single source of truth for application requirements.

- **Install ParchMint:** The [installation guide](install.md) covers release downloads and updates.
- **Use the application:** The [user guide](user-guide.md) explains the available workflows.
- **Understand the system:** The [architecture](architecture/architecture.md) maps
  crate responsibilities, data ownership, and the edit-to-save flow.
- **Change a component:** Each crate's `README.md` describes its interface and
  links to its implementation. Nearby unit and contract tests define behavior.
- **Verify a workflow:** The [UI driver](../tests/parchmint-ui-driver/README.md)
  exercises the production event loop and services with controlled native inputs.
  The [image tools](../tests/parchmint-ui-verification/README.md) capture and compare PNGs.
- **Keep good ideas:** [Future work](future-work.md) collects concise product and interface follow-ups.
- **Build and test:** The repository [README](../README.md) lists the pinned,
  locked development commands.
