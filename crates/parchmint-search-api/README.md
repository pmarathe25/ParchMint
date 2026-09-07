# `parchmint-search-api`

**Purpose:** Define project-wide search over a disposable index of project files
and open editor sessions. Local Find belongs to each editor view. Global
replacement uses hits as candidates; the application rechecks and applies edits.

## Interface

`SearchIndex` opens or rebuilds an index, replaces or deletes document text
snapshots, streams queries through `SearchBatchSink`, cancels query generations,
and verifies integrity. `SearchHit` carries source identity, revision, and range.
See [lib.rs](src/lib.rs).

## Query and result rules

Indexed fields are body text, display title, Synopsis, and project-defined
metadata. Callers supply known field names and ordinary search text. The
implementation constructs database queries; field-specific ranking is not
implemented.

Possible matches are checked against case-sensitive and Unicode whole-word
rules. The application verifies each hit's revision and range against current
files or an open editor session before navigation or replacement.

Results arrive in small batches tagged with a query generation. Starting a query
cancels its predecessor; late batches from that predecessor are ignored. Missing,
corrupt, or incompatible indexes are rebuilt from project data. Search runs on a
worker, has no network access, and cannot change project files or save state.
