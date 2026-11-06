# Project assistant and a single editor

Status: design proposal, revised October 30, 2026. The first implementation is
in progress; this document describes the intended design, not a guarantee that
every feature below is implemented. The interactive prototypes use fictional
content and simulated results; they make no OpenAI requests.

The current implementation provides the single editor, Lore section, assistant
dock, provider settings for OpenAI, Anthropic, and compatible endpoints, bounded
text-based context retrieval, continuity and research requests, and requested
access to notes. Research can save a summary,
links, or an unchanged original. Bounded UTF-8 text, Markdown, HTML, and XHTML
originals can be searched locally; PDF original-file contents are not yet indexed.
Check can display bounded findings whose quotations are validated against the
captured evidence, save author decisions with project undo and redo, and propose
revision-checked replacements for eligible single-paragraph passages. Each
assistant mode supports multiple locally persisted conversations.
OpenAI Ask and Research, Anthropic Ask, and opted-in compatible Ask show transient
streamed text; saved answers, citations, and actions still require a completed,
validated response. Additional context controls, derived lore indexes, richer
edit proposals, Anthropic Research streaming, and
some navigation and note-targeting behavior described below remain future work.
Consult the user guide for currently supported workflows and limitations.

A 23-case synthetic story corpus exercises the production retrieval path with
aliases, chronology, beliefs, group targets, exclusions, and distractors. It
reports evidence and complete-bundle recall separately; it does not measure
model answer truth or unsupported claims. In the latest offline run, 21 of 22
permitted qualifiers were retrieved; the missed case requires a semantic link
between a statement and its speaker. One crowded-context qualifier previously
missed is now selected by the document-diversity preference. Run it with `cargo test --locked -j 1
-p parchmint-desktop --lib story_corpus_reports_retrieval_recall_and_scope_separately
-- --test-threads=1 --nocapture`.

PDF extraction remains separate work. Parse untrusted PDFs in a cancellable,
resource-limited helper process, with input and output limits and no network
access; a per-stream decompression limit alone does not bound parser memory.
Record the extractor version and page coverage, and report unsupported, scanned,
encrypted, or partially extracted pages instead of presenting incomplete text as
a complete source. Use the same extraction and evidence-validation policy for
retrieval and continuity decisions, and avoid holding the project workflow lock
while a helper parses. Continuity evidence is now validated outside that lock;
the dispatcher rejects the decision if edits or History/recovery restoration
change the captured project before publication. Preserve original bytes and revalidate their identity
before accepting evidence. The helper needs platform-specific resource limits
and must fail closed if those limits cannot be established.
An initial containment experiment should evaluate a bundled WebAssembly parser
without WASI imports, using bounded linear memory, execution fuel, and a parent
timeout. Its interpreter and host allocations would still need independent
limits; linear-memory limits alone are not a hard process-memory limit. Verify
parser compatibility and packaging before selecting this approach.
An isolated November 4 probe built lopdf 0.47.0 with default features disabled
using the pinned toolchain and a locked WASI Preview 1 build. The optimized
1.9 MB guest extracted text from a tiny synthetic PDF under a local WASI host
with no preopened directories. Its imports were `random_get`, environment
queries, `fd_read`, `fd_write`, and `proc_exit`; the unoptimized guest exceeded
the host's function-local limit. This establishes basic build compatibility
only: the probe had no fuel or host-memory limits and did not test untrusted
PDFs, page coverage, packaging, or application integration.
The same guest subsequently ran in an isolated `wasmi` 2.0.0 host with module
validation, portable dispatch, a 64 MiB linear-memory limit, fuel, no
preopened directories, an import allowlist, and bounded in-memory output. A
synthetic page extracted correctly; insufficient fuel, an insufficient initial
memory limit, an output cap, and a 16 MiB compressed page rejected execution.
Portable dispatch was necessary in the debug probe to avoid host stack
overflow. This does not bound the interpreter's total host allocations or prove
page coverage, process cancellation, packaging, or safety against hostile PDFs.

## Product decision

Replace the companion editor with a project-aware assistant in the right dock.
Keep one tab strip and one document editor. Manuscript, Lore, and Research
documents open in that same editor. The assistant helps the author remember, investigate, and
review their work while the manuscript remains the primary surface.

The current prototype is **Minimal**. It replaces the earlier three variants
with one quieter panel: a flat row of Ask, Check, Research, and Notes tabs,
plain content, and one primary action for each task. Earlier prototypes are
retained as design history rather than additional product surfaces.

## Workspace and navigation

- Keep the navigation rail, Explorer, formatting toolbar, document tabs,
  breadcrumbs, focus mode, and status bar.
- Replace the split-editor button with a labeled **Assistant** toggle. Remove
  companion terminology, split ratio, pane-focus actions, move-to-other-pane
  menus, and tab/Explorer drop targets for a second editor.
- The right dock has one flat row: **Ask**, **Check**, **Research**, **Notes**.
  Notes is a separate view of the existing project annotations. It is not an
  additional agent mode. The existing Notes status control selects that tab.
  Switching tabs preserves both the Ask composer and any note draft and target.
- Reuse the existing status-bar Notes control and Settings navigation; do not add
  a second Notes button, a second Settings page, or duplicates of existing
  editor controls. Add an **AI assistant** category in the existing Settings
  application section, alongside Appearance, Keyboard shortcuts, and Dictionaries.
- When the selected provider is unconfigured, Ask, Check, and Research remain
  open and display “No AI API key has been set up” with an **Open Settings**
  button leading to Settings → AI assistant. Keep the selected tab and drafts
  so the author can return after setup. Notes remains available with no provider. Missing credentials,
  unavailable service, unsupported capabilities, and local providers requiring no
  authentication are distinct states, not all “missing API key”.
- Add **Lore** as a first-class project section beside **Manuscript** and
  **Research**. All three support documents, groups, ordinary editing, search,
  notes, undo, recovery, and History. Lore and Research are excluded from
  manuscript word totals and default manuscript export.
- In Assistant, use exactly **Ask**, **Check**, and **Research** modes. Keep a
  conversation's context and composer when switching modes or documents. Each
  mode supports multiple locally stored conversations, with a compact selector,
  creation, rename, and deletion. Changing the
  visible document does not silently change an already-submitted request.
- Keep the default panel sparse. Remove the duplicate Assistant heading,
  repeated context descriptions, suggestion lists, fact-count badges, colored
  result cards, and persistent usage footer. Ask shows conversation and a
  composer; source lists and context controls expand only when requested. Usage
  and spending limits live in Settings rather than in the writing panel.
  Check starts with one target control and Check action; finding evidence and
  edit actions expand within a selected finding. Research starts with its
  question and Search action; import form and provenance appear within a source.
  Notes shows existing notes, their passage links, and Add note.
- Proposed defaults at 1440px: 48px rail, 240px Explorer, 360px assistant,
  remaining width for the editor. Resize the assistant from 300–520px; preserve
  at least 480px of usable editor width. Collapse Explorer before shrinking the
  manuscript below that width. At compact native widths or large application
  zoom, open the dock as a dismissible overlay and restore editor selection on
  close. Focus mode hides it. The inline prototypes stack at phone widths solely
  so the designs remain inspectable in chat.
- F6 visits the visible rail, Explorer, editor, and dock. Escape dismisses an
  overlay or menu without clearing a composer. Use the existing shortcut catalog
  for Assistant and Ask about selection; select conflict-free defaults during
  implementation rather than hardcoding another key handler.

## Three project sections and three assistant workflows

**Manuscript** contains the story as written. **Lore** contains the author's
characters, places, world rules, relationships, chronology, and background plans.
**Research** contains external source material and derived reference notes. Lore
is ordinary authored project content, independent of the assistant and usable
offline. The assistant's extracted entities, facts, and embeddings are derived
indexes of these documents; they do not become a second authoritative story bible.

| Mode | Primary behavior | Durable result |
| --- | --- | --- |
| Ask | Answer questions about anything in the project, retrieving from all three sections | Optional saved answer or a reviewed addition to Lore |
| Check | Automatically examine a specified document or group against Lore, Research, and the rest of the plot | Evidence-linked findings and author decisions |
| Research | Find new sources, preview how they will be incorporated, and add them to Research | Original sources, summaries, or distilled notes with provenance |

### Ask and context

Default to **Project context · Automatic** in every mode that uses project
material. Its details list the selection, current document revision, actual
retrieved documents from Manuscript/Lore/Research, pinned sources, and excluded
material. Retrieval may read other chapters and saved Research without requiring
the author to open or attach them. Allow explicit selection-only, current document,
chosen group, and project retrieval. Excluded documents are never retrieved
implicitly. A
per-project instruction can express genre, tone, canon rules, and whether later
chapters may be consulted. Reader-knowledge cutoff is separate from story time.

Answers cite manuscript passages and mark interpretations or unknowns. A link
opens the passage in the single editor; **Back to writing** returns to the
originating document and selection. Small quoted previews let authors inspect
evidence without navigation. Research documents remain browsable in ordinary
editor tabs. The assistant never becomes the only way to read reference material.

### Notes and requested agent actions

Keep the existing anchored notes as the single source of truth. The Notes tab
reads and edits those records across Manuscript, Lore, and Research; the assistant
does not create a separate collection of AI notes. Every note retains its
document and passage link. Unsubmitted note drafts stay private UI drafts until
the author commits them.

Expose `read_notes` and `create_note` through the assistant port when the human
requests the corresponding action. Examples: “Read the notes on this scene”,
“Summarize the notes across the project”, and “Add a note here: check the tide”.
General Ask and Check requests do not silently include private annotations.
Explicit requests to consider notes can add their contents to that operation's
context within its allowed scope and exclusions.

For creation, resolve the requested document/passage or capture the current
selection/caret anchor before any network work. Validate its revision on return
and commit through the ordinary annotation command path. If an anchor cannot be
resolved or has become invalid, preserve the proposed note and ask for its target;
never attach it to whatever document happens to be active when a response arrives.
When the request supplies the body and target, create the note directly and
report the destination with an Open note link. The user's request authorizes this
reversible action; an additional approval dialog is unnecessary.

Return the created note ID and preserve ordinary Undo, recovery, autosave, and
History. Use the originating operation ID to avoid duplicates after retry.
Existing notes and unsent manual drafts survive tab switches, source navigation,
failures, and agent completions. Do not edit or delete existing notes merely
because the agent can create one. Text inside retrieved notes is source material,
not authorization for further actions.

### The Lore section

Example outline: Lore → Characters → Mara Venn; Lore → Places → The harbor;
Lore → World rules → The keeper's key. These documents open in the same editor
as a manuscript chapter. The author can write and revise them without an AI call.
Ask may propose a Lore addition, but saving requires a visible destination and
reviewed text. There is no Lore assistant mode or compulsory fact-by-fact
approval workflow. Authorial Lore takes precedence as intended fictional canon;
contradictions within Lore are still review findings, not silently resolved.

Derive explicit lookup records rather than treating the chat transcript as memory:

- **Entity:** stable ID, type, display name, aliases, author notes, relationships.
- **Fact:** subject/predicate/value, story-time interval, truth perspective
  (world fact, belief, rumor, author-only plan), canon status, and source anchors.
- **Event:** participants, location, story-time interval, ordering constraints,
  and source anchors. Reading order and story chronology are independent.
- **Evidence:** project/document/block IDs, captured revision, quoted text,
  fingerprint, and source kind. External sources also record URL, page title,
  retrieval date, and a bounded excerpt or summary.

Distinguish authored Lore, manuscript evidence, outside Research, and unaccepted
assistant suggestions. Draft proposals stay outside Lore until the author saves
them there. Brainstorming does not become canon automatically. New manuscript
evidence can flag a conflict with Lore but cannot overwrite it. Disputed or
outdated index entries retain their sources. Entity alias merging requires a
reviewable choice.
Keep alternate timelines and character beliefs from being flattened into facts.

For example, “Mara lost her left eye in the siege” has a source and a time range.
“Mara watched him with both eyes” becomes a possible inconsistency only if its
scene occurs after the siege and the wording is literal. A flashback or unreliable
narrator may explain it; the author decides.

### Continuity checks

Select a **document or group to check**, then run an automatic consistency pass.
The author does not have to choose each comparison document. The target is the
content being examined; evidence may come from all allowed sections, including
outside the selected group. Group checks examine their documents against each
other as well as outside evidence. A target may be in Manuscript, Lore, or
Research. An explicit evidence restriction is separate from target selection.

Run on explicit request in the first version. Optional **Recheck after edits** is a
later, per-project opt-in with a visible cost ceiling, debounce, and coalescing.
Local indexing can follow edits without making paid calls. Avoid paid work on
every keystroke and unsolicited interruptions while writing.

A finding has a stable ID, category, short explanation, both evidence anchors,
the checked revisions and scope, and a decision: Open, Intentional, Dismissed,
Resolved, or Needs recheck. Categories include identity/appearance, time/travel,
objects/possession, rules, relationships, and who knows what. These are suggestions,
not proof of completeness or objective errors. A scan reports exactly which
documents and fact categories were checked and any unprocessed material.

Offer **Show passage**, **Mark intentional**, and **Propose edit**. An edit first
shows a before/after patch against a captured revision. **Apply** validates the
revision and anchor, then uses the owning application/editor command lane. It
is undoable and participates in ordinary autosave, recovery, and History. If the
text changed, disable Apply and offer Recheck. Resolving a finding is separate
from approving a canon fact. Keep decisions tied to evidence fingerprints so an
unrelated edit does not resurrect a dismissed item.

Lore represents intended fictional rules; Research supplies evidence about the
outside world; Manuscript shows what the story actually says. A historical
departure can be deliberate worldbuilding. Findings identify the kind of conflict
and its sources instead of treating every Research statement as binding canon.
When Lore changes, invalidate dependent checks and retrieve the new text.

### Research

Web access is enabled for a research task, not silently for every lore question.
Show the proposed external query and let the author remove names or private
details before running it. Default to sending a generalized question, without
manuscript excerpts. Any manuscript context included in a web-enabled model
request must be visible in its context review.

Use the selected provider's web-search capability or an explicitly configured
search service, and render its URL citations as clickable links. The OpenAI
adapter can use the Responses API web-search tool. If no search capability is
configured, offer manual source import. Separate sourced findings, conflicting sources, and the
assistant's interpretation for the story. Saving creates an ordinary document in
Research with its question, answer, source links, date, and provenance. It does
not approve fiction canon. Before adding a source, choose the destination Research
group and one of three forms:

- **Original source:** preserve the original supported document/file and its
  provenance without an AI rewrite. A readable extraction may accompany an
  original attachment; it must be labeled as an extraction, not an unchanged
  copy. Keep original bytes when supported and available for import; otherwise
  retain a source link and explain the available form.
- **Summary:** a condensed account of the source, with source links and enough
  attribution to distinguish summary from quotation.
- **Distilled notes:** selected facts, excerpts, or practical takeaways tailored
  to the project, with each retained claim traceable to its source.

The import preview shows title, destination, original/derived form, source URL or
file reference, and access date. Derived forms retain a reference to their source;
re-importing a source does not overwrite authored notes. Failed retrieval or an
unsupported format keeps the draft and cannot produce a fake successful import.
An original import uses the ordinary document/import path rather than spending
tokens rewriting the content. Source availability and import permissions apply
to the actual source. Sources are untrusted input and cannot authorize
document writes or additional data access. Model tools never receive a general
shell or unrestricted filesystem access.

## Architecture and ownership

Follow the existing API/implementation split. Names below are proposed new crates.

```text
ui-iced: dock, threads, context picker, citations, review UI
    -> ui-api: session-scoped assistant workflow port
    -> application: snapshots, context policy, approved mutations
    -> assistant-api: framework-neutral requests/events/tool contracts
         -> assistant-core: retrieval, bounded tool loop, proposals, scan jobs
         -> assistant-openai: HTTPS Responses/SSE, structured results, usage
         -> assistant-compatible: configurable compatible endpoint adapter
         -> other provider adapters: provider-native protocol and capabilities
         -> assistant-index-sqlite: rebuildable entity/chunk/evidence lookup
    -> platform-api/native: credential-store capability
desktop: adapter assembly, cancellation, project/window lifecycle
```

Do not place network calls or prompt orchestration in an Iced view. Workers emit
typed events carrying project/session/window IDs, request ID, generation, and
the captured document revisions. Switching documents may let a request finish in
its original thread, but it must never insert text into the newly active document.
Closing the project cancels jobs and ignores late completions. Stream text and
explicit tool activity; provide Stop. Do not expose hidden reasoning.

The minimum read tools are `search_project`, `read_passages`, `read_lore`, and
`read_timeline`, with `read_notes` for requested note access. All are enforced by
application context policy. Tools return bounded data with citations. A
`propose_change` output stages manuscript/Lore edits; it cannot commit them.
`create_note` is separately authorized by a human request and commits a validated
annotation through the existing owner. Validate every structured result, cited ID, range, and revision in
application code. Unknown IDs or unsupported claims are presented as unverified.

### Retrieval and freshness

1. Take immutable snapshots from the existing document owner, including unsaved
   edits when the author included them. Never rely only on saved disk content.
2. Chunk along paragraph/block boundaries, preserving stable IDs and revisions.
3. Index all allowed Manuscript, Lore, and Research documents. Combine the
   existing FTS5 search with alias/relationship expansion and relevance ranking.
   Evaluate semantic retrieval against paraphrased and indirect references;
   add local vectors if needed to meet retrieval acceptance criteria. Account
   for any embedding requests separately.
4. Retrieve a bounded number of relevant chunks, authored Lore, and event
   constraints. Pin explicit context and obey context/exclusion limits first.
5. Generate a cited answer or structured candidates. Summaries are navigation
   aids; retrieve source evidence before reporting a contradiction.
6. Invalidate derived chunks on document edits/deletes/restores. Recompute only
   affected dependencies. Surface stale evidence rather than guessing a new
   anchor. Whole-project checks build coverage incrementally and expose progress.

### Persistence

| State | Proposed owner/location | History policy |
| --- | --- | --- |
| Manuscript, Lore, Research, author notes, intentional decisions | Versioned project resources declared in project.toml; Lore is ordinary documents under its section | Included; ordinary undo and recovery |
| Imported source originals and provenance | Validated project resources with original/derived links; referenced by Research documents | Included in project backups and documented History/restore scope |
| Derived chunks, embeddings, extraction candidates, scan cache | `.parchmint/cache/assistant.sqlite` | Rebuildable; excluded |
| Conversations, unsent composers, unsaved proposals | Application-data store keyed by project ID; durable local journaling | Excluded until explicitly saved into the project |
| Dock width, mode, current thread, return-to-writing location | workspace-state; references only | Excluded |
| API credential | Native OS credential store, referenced by preferences | Never project, logs, History, or export |
| Usage journal and budget reservations | Application-data store shared across local project windows | Excluded; transactional |

All new authored resources need explicit format contracts, backup/export
behavior, schema versions, and restore semantics. A cache failure must not erase
Lore documents. A project copy includes authored Lore and Research documents but
does not copy private conversation history or credentials. Restoring older
manuscript text invalidates affected assistant evidence; authored Lore restores
only under its documented project/document restore scope.

## Provider configuration, privacy, and the $100 allowance

Settings → AI assistant owns the selected provider profile: provider/protocol,
model identifier, optional custom endpoint, authentication method, secret-store
reference, and optional provider-specific budget. Include OpenAI and an
OpenAI-compatible endpoint as initial adapters; design the interface for native
Anthropic, Gemini, and other provider adapters. A configurable base URL does not
guarantee compatibility with every API. Provider support is capability-driven:
check model availability, streaming, tool calling, structured outputs, usage
reporting, and web search independently. Chat and project retrieval should remain
useful when an optional capability is unavailable. If web search is unavailable,
Research explains the limitation and offers an explicitly configured search
provider or manual source import; it never invents sources or silently switches
providers. A local endpoint can explicitly opt into no authentication.

Keep profiles and non-secret preferences in the existing process-wide preference
coordinator. Add a new credential-store port to platform-api/native; this service
does not exist today. Store one secret per profile through OS secure storage, with
a session-only fallback if unavailable. Entering, replacing, and removing a key
must never put it in the project, preferences, History, logs, test snapshots, or
model context. Mask the field and never reveal a saved secret. When switching
providers, do not send one provider's key to another endpoint. Allow HTTPS custom
endpoints and explicit localhost HTTP for local services. Validate endpoints and
redirects before attaching credentials. A connection test is optional, is labeled
as a network request, and preserves pending input on failure.

Use the author's own API key, entered through Settings → AI assistant, stored using
Windows Credential Manager, macOS Keychain, or an appropriate Linux secret store.
If a secure store is unavailable, offer session-only entry. Never ship a shared
key in the desktop executable. A backend is unnecessary for the personal-key
version. Treat a desktop key as the user's credential, not as an extract-proof
secret; it is accessible to its owner and authorized local processes.

Use Responses with streaming and `store: false`; maintain conversation history
locally and supply bounded context explicitly. API content is not used for
training by default. `store: false` is not Zero Data Retention: standard abuse
monitoring can retain content for up to 30 days, and account-specific controls
apply. Explain this once during setup, alongside the chosen project scope.

Start with configurable GPT-6.1 Terra for normal work and evaluate GPT-6.1 Sol
for difficult checks. Treat these as initial candidates, not a quality guarantee;
pin supported snapshots for releases and maintain a capability/pricing table.
The fetched model page confirms Terra supports streaming, function calling,
structured outputs, and web search. Use the higher-cost route only on request or
within an author-enabled routing policy.

At the October 29, 2026 published standard rates, Terra costs $1 per million
input tokens and $5 per million output tokens. A hypothetical turn with 12,000
input and 1,500 output tokens costs $0.0195; 1,000 such turns cost $19.50 before
tool fees and extra reasoning/output usage. Web search adds $0.01 per tool call
plus search-content tokens. This is arithmetic for a fixed workload, not a
forecast of manuscript-wide review costs or a promise about account eligibility.

For the user's OpenAI profile, a suggested starting policy is an **$80 monthly
ParchMint limit**, warning at $60,
and a $20 margin against the user's stated $100 allowance. The app must display
**Estimated ParchMint usage**, not “credit balance”. It cannot know spending from
other programs or devices. Make the billing period/reset day configurable to
match the credit grant; do not assume all monthly credits reset on the first.
Other provider profiles have separate budgets, currency, billing periods, and
pricing. When pricing is unknown, show token counts and an unavailable estimate;
do not reuse OpenAI's rates or present a false hard dollar cap.

Compatibility references: [Anthropic's compatibility layer](https://platform.claude.com/docs/en/cli-sdks-libraries/libraries/openai-sdk)
and [Gemini's compatibility documentation](https://ai.google.dev/gemini-api/docs/openai/)
describe provider-specific limitations, so a base URL alone is not a guarantee.

Before each paid job, reserve a conservative request budget in a journal shared
by all local windows. Cap total input/output tokens, tool calls, recursive steps,
job spend, and concurrency. Reconcile actual usage including cached tokens,
reasoning/output tokens, search calls, and failures where usage is available.
Stop dispatch when reservations would exceed the local limit. Interrupted or
ambiguous requests retain an unresolved reservation; avoid automatic replays that
could double-charge. A cancelled request may still incur costs. Account-level
billing remains authoritative; this local guard cannot ensure a universal hard
cap across devices, external clients, or changed provider rates.

Offline or unconfigured: writing, authored Lore, saved research, and prior
conversations remain readable. The dock shows a clear connection/setup state;
the composer draft survives failures. A 401 opens a credential repair action,
429 uses bounded backoff, and network errors expose Retry. Explicitly retry an
ambiguous paid submission instead of silently sending it again.

## Removing the companion pane safely

Collapse saved views into one tab strip, preserving order and a valid active
document. Deduplicate only multiple views of the same saved document; preserve
distinct draft IDs even if their titles match. If the companion was active,
select that document and its cursor/scroll record. Recovered drafts remain
protected before the old pane state is dropped. Keep the previous layout record
until the conversion is durably saved; a failure leaves it available for recovery.
Document/editor undo belongs to document sessions and must not be discarded.

Implementation affects ui-iced's editor workspace/rendering, harness targets,
workspace-state, preferences command catalog, user guide, contracts, driver flows,
and visual references. Remove research-to-companion routing and pane-specific
search/focus/navigation assumptions together. Do not simply relabel the old pane
or turn it into an assistant that still owns manuscript tabs.

## Implementation sequence and acceptance

1. First-class Lore root, single editor, and shared Notes/Assistant dock;
   layout conversion and recovery. Extend section/format contracts, Explorer,
   Overview section selection, creation/save destinations, search, drag/move,
   History, and export exclusions together. Existing Manuscript and Research
   content keeps its identity; adding Lore does not reclassify existing notes.
   Minimal assistant shell with separate Notes tab, setup, expandable context,
   threads, streaming, Stop, and usage accessible through options/settings.
2. Cited project Q&A using existing FTS and immutable live snapshots. Ship only
   after retrieval/grounding evaluation on realistic projects.
   Add requested note reading/creation through the existing annotation services;
   preserve manual note drafts and return created note IDs.
3. Derived Lore indexes, reviewed document additions, evidence invalidation, and timeline.
4. Document/group consistency scans against project-wide references, evidence
   review, and revision-checked patch proposals.
5. Cited web research with original, summary, and distilled-note import. Optional
   after-save checks and embeddings follow measured need.

Required behavior checks: no lost saved tabs or drafts during pane removal;
late response after tab switch/project close; edit/restore/delete during a scan;
unsupported citation; intentional flashback; conflicting aliases; Lore edits
invalidating checks; group targets with outside-group references; original imports
preserving source bytes; summary/distilled provenance; manuscript counts excluding
Lore and Research; offline/401/429/cancellation; simultaneous budget reservations;
unknown billing outcome; stale patch; Undo/Redo/save/reopen of accepted changes.
Also verify requested note reads obey scope, ordinary questions do not consume
notes, explicit note creation is singular and anchored, retries do not duplicate
notes, stale anchors retain proposals, and manual drafts/old notes survive agent
completion and source navigation.

Create a small authored evaluation corpus with expected evidence and uncertainty,
including apparent contradictions that are valid flashbacks, rumors, and
character beliefs. Measure retrieval recall and unsupported claims separately.
Continuity checks must not promise exhaustive coverage.

Native verification when the feature is implemented: both appearances at
1280×720 and 1440×900, 100/125/150% zoom, reduced motion, keyboard focus, resizing,
streaming while typing, Notes/composer preservation, passage navigation and return,
patch/undo/save/reopen, budget stop, and interrupted-session recovery. Follow the
repository's UI-review procedure with disposable projects and serial locked builds.

## Prototype validation and limits

These are interactive design simulations, not native feature implementation.
Mock citations, budget figures, and research summaries are illustrative and
must not be mistaken for actual manuscript analysis, account usage, or research.
The running release editor was inspected in an isolated copied fixture to ground
the chrome. Desktop interaction stopped as soon as the user requested it; a full
native workflow review of the proposed feature is pending implementation and a
user-available desktop. No Rust compilation is needed for this design-only change.

The minimal prototype was verified headlessly at 1024, 736, 375, and 320px browser
widths in every tab, with no measured horizontal content overflow. Light and dark
screenshots were inspected. Exercised sample actions include requested note reads
and creation, existing-note and manual-draft preservation, document/project note
scope, empty/negated note requests, ordinary questions without note access, group
checks, edit apply/undo, and Research import in all three forms. Earlier prototypes
also exercised editable Lore and evidence invalidation. These checks validate
design simulations only, not native behavior or a live API integration.
The updated preview also checks the unconfigured Ask/Check/Research messages and
their Settings buttons, Notes without a provider, returning to the selected tab, and an unauthenticated
compatible local endpoint. Provider names and sample credentials in the preview do
not establish provider support or perform a connection test.

## Official references checked

- [Responses API](https://developers.openai.com/api/docs/guides/responses)
- [Web search and citation requirements](https://developers.openai.com/api/docs/guides/tools-web-search)
- [API data controls](https://developers.openai.com/api/docs/guides/your-data)
- [GPT-6.1 Terra capabilities](https://developers.openai.com/api/docs/models/gpt-6.1-terra)
- [Pricing](https://developers.openai.com/api/docs/pricing)

Repository grounding: README.md, docs/README.md, docs/user-guide.md,
docs/architecture/architecture.md, docs/design-language.md, and the component
guides for ui-iced, workspace-state, application, search-sqlite, desktop, and
design-system. Native and headless verification procedures informed the future
acceptance criteria; they have not been reported as feature verification.
