# Resource checkpoint attribution

Hidden benchmark diagnostic selectors enable one fixed, bounded resource-phase
row per presented checkpoint. Ordinary browsing performs no extra phase-clock
calls. The aggregate `style_refresh_ms` includes more than element and pseudo
cascade work; it is not an isolated CSS selector benchmark.

The diagnostic row separates:

- HTML resource discovery and admission;
- media-source selection;
- linked/embedded stylesheet dependency discovery;
- cascade/cache refresh;
- computed background/mask image discovery;
- embedded-image decoding;
- visible-text font subset selection;
- inline SVG serialization, version comparison and changed-raster decoding.

Each phase reports elapsed time, not exclusive CPU time. Cascading may occur
again during lazy style hydration in resource discovery. Font selection does
not include the later background network fetch. SVG time includes unchanged
input checks and changed raster work, not just the third-party decoder.
The separate layout clock still covers subsequent box construction, shaping,
geometry observers and painting. These diagnostics do not change scheduling,
resource eligibility or presentation barriers, and carry no author URLs,
attribute values, font names or source text.

Timers reset at every checkpoint, including retained Canvas-only checkpoints.
An unchanged inline-style proof reports only its actual cascade work. A failed
proof retains its first cascade comparison and records the resource fallback
without recalculating the cascade or reusing the previous checkpoint's timings.
Layout snapshots have their own disabled profiler and do not inherit a Page's
diagnostic state. Draining a row is destructive, not a cumulative speed counter.

Rendering resource discovery uses the existing non-script discoverer. Preparing
and cloning inline script source merely to discard the resulting script list
is unnecessary here: the authoritative parser and runtime own script preparation
and execution. Dynamic script/CSP/lifecycle behavior remains on those paths.

Keep optional V8 stack sampling disabled for ordinary speed comparisons. Even
these lighter phase diagnostics add overhead and should be used to attribute
costs; confirm changes using repeated unprofiled release runs with matching
viewport, cache state and an observable completion endpoint in both browsers.
