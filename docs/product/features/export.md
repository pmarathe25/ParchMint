# Export

- **EXP-001:** v1 exports one self-contained HTML5 manuscript.
- **EXP-002:** v1 always exports the entire Manuscript. Partial scope is deferred.
- **EXP-003:** Title emission uses Inherit/Enabled/Disabled at the project, group, and document levels. Group and document page breaks are Enabled or Inherit; the project page-break default is Enabled or Disabled. Per-node inclusion overrides are deferred.
- **EXP-004:** Numbering is an export-run option. v1 does not persist arbitrary numbering for individual nodes.
- **EXP-005:** Group titles may emit headings despite groups having no body.
- **EXP-006:** The exporter does not duplicate existing document title content.
- **EXP-007:** v1 export excludes Research, comments, Synopsis, and metadata.
- **EXP-008:** The export interface identifies Entire Manuscript as fixed v1 scope and contains output path/name, title/page-break controls, numbering, and Export.
- **EXP-009:** After export, the user may open the result or reveal it in the file manager.
- **EXP-010:** Generated TOC and in-app preview are deferred.
