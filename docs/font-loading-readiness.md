# Font loading and renderer readiness

`document.fonts.ready` promises a settled font environment, not merely a
completed download. The contract follows [CSS Font Loading Level 3](https://drafts.csswg.org/css-font-loading/#fontfaceset-pending-on-the-environment).

## Environment ownership

The renderer owns document lifecycle, stylesheet requests, automatically
requested CSS fonts, and publication of layout metrics. Its private font
environment remains pending while any of these are outstanding:

- Document loading has not completed.
- A stylesheet request is pending.
- A used CSS font is still being fetched or decoded, including fallback candidates.
- A real layout provider has not published geometry after a font or document change.

The initial ready promise is fresh and pending even when the set is empty.
An empty, fully loaded document fulfills it without manufacturing a font
loading event. An unused CSS face remains `unloaded` and does not trigger a
download just to fulfill readiness.

Downloaded font bytes are decoded and installed before successful face
completion. The renderer invalidates font-dependent geometry and acknowledges
the revision when layout is published. A pure script runtime without a layout
provider does not wait for a nonexistent renderer.

Container reconstruction is not successful font admission by itself. SFNT
directories must be complete and table bounds valid; duplicate table tags are
rejected. On Windows, the existing Swash parser also requires usable head,
glyph-count, horizontal-metric and character-map tables before registration.
These are bounded admission checks, not a claim of full OpenType sanitization.
The text backend still parses shaping and raster tables defensively. See the
[OpenType file structure](https://learn.microsoft.com/en-us/typography/opentype/spec/otff).
Buffer parsing failures reject with `SyntaxError`; exhausting URL candidates
rejects with `NetworkError`. Neither failure installs a font or blocks ready
forever. Success fixtures use the existing licensed Ahem font, not dummy tables.

## Tasks, promises, and fast responses

Font completion uses the private font loading task source. Fetch promise
reactions alone do not publish a successful `FontFace`, settle its set's loading
period, or dispatch completion events.

The renderer remembers admitted requests for the current bounded CSS-connected
face list. This matters when a response finishes before the next JavaScript
checkpoint: the face still enters `loading`, settles its `loaded` promise, and
participates in the normal completion/error events. An immediate transport
failure must not leave `face.loaded` pending forever.

The native and JavaScript views share the same deduplicated admission list,
subject to the existing 64-rule limit (which can expose fewer distinct faces).
Request history is pruned against live rules rather than accumulating
every URL visited by the document. Loading identity includes the family,
descriptors, source and fallback alternatives: downloading a shared URL for one
face must not change an unused face's status or include it in completion events.
Installed CSS registrations are matched against the face's descriptors, rather
than assuming that a successful URL loaded every face that references it.
The admission limit is an implementation resource limit, not a claim of
unlimited CSS font support.

Script and Canvas-initiated URL loads use the normal Fetch stream machinery,
with destination `font`, anonymous CORS, and the committed document/worker
policy. They select `font-src`, not `connect-src`, including on redirects.
The private bootstrap handoff is removed before author execution, and public
fetch options cannot select this destination. Captured request serialization
and font loading functions keep author method replacements out of the internal
loading path. This follows [CSS font fetching requirements](https://drafts.csswg.org/css-fonts-4/#font-fetching-requirements).

When a loading period settles, its original ready promise fulfills before the
queued `loadingdone` event. Failed faces also participate in `loadingerror`;
ready itself never rejects. A subsequent load creates a new ready promise,
without retracting the fulfilled promise from an earlier period.

Dedicated workers own independent FontFaceSets and font registries. Their
initially empty sets do not depend on the creator's document or layout. Canceling
a document or worker discards pending font tasks.

## Scheduling and regression coverage

Environment notifications are private embedder hooks captured before author
scripts run. A pending network request does not install a polling timer.
The renderer schedules a checkpoint only when an environment transition or
queued font task needs service. Author replacements of public prototype methods
or timer functions cannot forge or cancel those notifications.

Regression coverage includes:

- Pending parsing, stylesheets, unpublished layout, and cancellation.
- Fast successful and failed CSS responses before the first script checkpoint.
- Real Ahem font decoding and Canvas/layout advance publication.
- Canvas-only measurement, fill, and stroke loading; assignment and empty or
  invalid painting do not fetch. Weight and Unicode-range matching select faces,
  repeated use shares the pending request, and worker fonts remain realm-owned.
- Missing and corrupt fallback candidates before a successful final source.
- Unused faces, deduplicated rules, and bounded live admission history.
- Fonts discovered after document load and old/new ready promise identity.
- Document and worker task boundaries and event ordering.

The pinned, unmodified CSS Font Loading WPT cases are additional evidence for
their specific assertions, not proof of full Font Loading or CSS Fonts
conformance. Variable-font descriptors, local font source selection, and other
unsupported behavior must be assessed separately.
