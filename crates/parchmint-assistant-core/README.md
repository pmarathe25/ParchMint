# Assistant core

Bounded, read-only lexical retrieval over authored project snapshots. It
includes Manuscript, Lore, and Research, honors pinned and excluded documents,
and does not include notes. The result records revisions and whether candidate
passages were truncated. Text matching is not an exhaustive continuity check.
Question matches lead the ranking, so repetitive target text does not crowd out
the strongest passages matching the author's question. An optional two-passage
outside-evidence reserve can surface name-only matches
from identity hints selected by the application. These hints do not establish
canon or merge entities, and all scope exclusions apply before selection.
Two-letter terms such as Li can match; common function words are ignored
case-insensitively, so a name such as An needs additional context to retrieve.
After ranked evidence and requested notes, spare capacity can include up to two
immediately adjacent authored paragraphs around direct text matches. This pass
is bounded, preserves exact anchors, does not cross structural breaks, and does
not run for selection-only requests. Adjacency does not establish a relationship.
Up to four outside-context slots prefer passages from otherwise unrepresented
documents, while ordinary direct question matches remain ahead of expansion-only
matches. This is a bounded coverage preference, not an exhaustive scan.
The local original-text extractor accepts bounded UTF-8 plain text, Markdown,
HTML, and XHTML without altering source bytes. HTML extraction does not render
styles or run scripts. PDF requires separate extraction support.
Extracted text shares the same retrieval limits and carries the immutable source
identity, hash, and segment number.

Check reports are parsed as bounded, untrusted JSON. Each quoted finding is
validated against the captured request passages, must include the check target,
and receives its document identity and navigation anchor locally. Ambiguous or
invented quotations never become trusted findings.
