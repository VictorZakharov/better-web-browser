# ADR 0001: Renderer process isolation boundary

- Status: Accepted; renderer-only top-level document path implemented, with site/frame isolation still pending
- Date: 2026-08-14
- Tracking issue: [#6](https://github.com/VictorZakharov/better-web-browser/issues/6)

## Decision summary

Breeze will treat all remote document content as hostile and run it outside the privileged browser
process. The first production allocation policy is one renderer process per top-level browsing
context. The policy can later choose a process by site or HTML agent-cluster rules without changing
the browser/renderer protocol.

The browser process is the trusted broker. It exclusively owns navigation policy and session
history, Fetch and WinHTTP, TLS, cookies and persistent storage, permissions, renderer lifecycle,
the Win32 shell, and final presentation. A renderer owns its documents, frames, HTML and CSS
parsers, JavaScript realms and event loops, DOM nodes, style and layout state, remote-resource
decoders, and display-list construction. The browser validates every renderer request and every
presentation update before acting on it.

Remote content never receives direct network, persistent-storage, filesystem, registry, clipboard,
device, process-launch, or native-window authority. The renderer will run in an AppContainer with no
capabilities and in a Job Object with explicit memory, process-count, and teardown limits before it
is allowed to parse remote content.

## Context

At the time this ADR was accepted, the Windows application kept the full page realm in
`BrowserState` and passed heap pointers through private `WM_APP_*` messages. The migration described
below has since removed that privileged fallback for remote pages: `Page`, `ScriptRuntime`, DOM,
style/layout state, decoded resources, Workers, and Reader extraction now live in the tab renderer.
The browser retains the Win32 shell, history, network and cookie authority, native controls, and
validated presentation painting.

Three completed foundations make a process boundary practical:

- [Issue #1](https://github.com/VictorZakharov/better-web-browser/issues/1) provides an explicit,
  deterministic event-loop scheduler and rendering checkpoints.
- [Issue #4](https://github.com/VictorZakharov/better-web-browser/issues/4) replaces pointer-derived
  DOM identity with stable `NodeId` values and mutation versions.
- [Issue #9](https://github.com/VictorZakharov/better-web-browser/issues/9) gives navigation and
  subresources one typed Fetch policy boundary, independent of WinHTTP.

The HTML Standard defines navigables, documents, event loops, and agent clusters, but it does not
prescribe an operating-system process model. Breeze therefore keeps specification identity separate
from process placement: a browsing context, frame, document, or origin is never identified by a
Windows process ID.

## Goals

- Keep the browser UI responsive when a renderer crashes, exhausts a budget, or stops servicing its
  event loop.
- Prevent a compromised renderer from directly reading user files or persistent browser state,
  opening network connections, launching children, or controlling browser chrome.
- Give navigation, Fetch, storage, presentation, cancellation, and teardown one typed, bounded,
  auditable protocol.
- Preserve stable logical identity across thread and process boundaries without transmitting raw
  pointers or process-local handles.
- Allow the process allocation policy to evolve independently from DOM, networking, and rendering.
- Make staged migration testable without calling an intermediate stage a security boundary.

## Non-goals

- This ADR does not deliver full site isolation, out-of-process iframes, Spectre defenses, or the
  HTML cross-origin-isolated capability. The initial renderer may contain all descendant frames for
  one top-level browsing context.
- It does not defend against a compromised browser process, an administrator, a modified Breeze
  installation, or vulnerabilities in the operating system.
- It does not add a GPU, media, extension, plug-in, accessibility, or download process.
- It does not require restoring arbitrary DOM or JavaScript heap state after a crash. Recovery
  reloads the last committed URL only after an explicit user action.
- It does not promise protocol compatibility between separately installed Breeze versions. Browser
  and renderer are the same build and reject an incompatible protocol during startup.
- This boundary does not make Breeze safe for sensitive browsing by itself; protocol and painter
  validation remain security-sensitive and the browser has not received an independent audit.

## Trust model

### Trusted components

The browser executable, its local configuration, and the Windows kernel security boundary are in
the trusted computing base. Browser-side protocol decoding, policy validation, Fetch, cookie and
storage services, and display-list validation must be written as security-sensitive code.

### Untrusted components

The renderer and every byte derived from a document are untrusted, even when TLS authenticated the
server. This includes HTML, CSS, JavaScript, URLs, response metadata forwarded to the renderer,
images, SVG, webfonts, generated display lists, native-control descriptions, diagnostics, and every
renderer-to-browser message. A renderer is assumed to become fully compromised.

### Threats and required controls

| Threat | Required control |
|---|---|
| Renderer opens files, registry keys, sockets, or devices | Capability-free AppContainer; all required data is brokered |
| Renderer launches or escapes through a child | Child-process mitigation plus a one-process Job Object with no breakaway |
| Renderer confuses authority between documents | Browser-issued context, frame, navigation, document, and request IDs; origin is browser-derived |
| Renderer sends malformed or oversized IPC | Fixed framing, bounds checked before allocation, exhaustive typed decoding, fail-closed session teardown |
| Renderer floods messages or requests | Per-session queue, byte, rate, and outstanding-operation limits with backpressure |
| Stale work mutates a replacement document | Every document-owned message carries the renderer session and `DocumentId`; cancelled IDs cannot be reused |
| Renderer crashes or hangs | Dedicated broker I/O, process/job monitoring, browser-owned error surface, bounded shutdown |
| Decoder exploit crosses the boundary | HTML/CSS/script/image/SVG/webfont decoding runs in the capability-free renderer |
| Presentation attacks the browser painter | Validate command count, finite geometry, buffer lengths, resource IDs, control count, and viewport clipping |
| Renderer abuses Fetch as a confused deputy | Browser reconstructs requests from allowed intent fields and applies origin, credentials, redirect, CORS, cookie, and body policies |
| Another local process connects to IPC | Only explicitly inherited anonymous-pipe handles; no discoverable endpoint name |

## Ownership

Ownership means the named side is authoritative. The other side can request an operation or cache a
revocable snapshot, but cannot silently create or mutate authoritative state.

| Area | Browser process | Renderer process |
|---|---|---|
| Browsing contexts and frames | Allocates IDs, owns parentage and process placement | Hosts assigned active documents |
| Navigation and history | Validates requests, allocates `NavigationId`, fetches, commits, traverses history | Requests navigation and reports same-document changes |
| Network | Owns Fetch policy, WinHTTP, proxy/DNS, TLS, redirects, auth policy, and response filtering | Emits bounded fetch intents and consumes filtered streams |
| Cookies and storage | Sole persistent owner; enforces origin/path/expiry/credential policy | Holds document-scoped snapshots and submits mutations |
| Permissions | Owns prompts, grants, revocation, and durable decisions | Requests a named capability for an active document |
| DOM and JavaScript | No direct node pointers or script execution | Owns documents, `NodeId`, realms, tasks, microtasks, and mutation state |
| CSS and layout | Supplies viewport and platform preferences | Owns cascade, invalidation, font metrics, layout, and scrolling geometry |
| Resource decoding | Forwards bounded response bytes | Parses and decodes images, SVG, and webfonts |
| Display lists | Validates revisions and commands, then paints/composites | Builds immutable updates from current layout state |
| Page controls | Owns HWNDs, OS handles, focus bridge, IME, and final clipping | Owns web-visible value/state and sends bounded semantic/geometry descriptions |
| Reader surface | Owns the chrome command and final presentation | Extracts remote content and builds its reader document/display list |
| Input | Receives native events and validates focus/coordinates | Dispatches DOM events and returns cursor/focus/control changes |
| Process health | Launches, monitors, accounts, terminates, and exposes task-manager metrics | Sends readiness, progress, diagnostics, and cooperative shutdown acknowledgements |

Persistent cookies and storage remain browser-owned even if a renderer receives a local cache for
synchronous web APIs. A cache is scoped to one origin and document generation; the browser sends
updates or invalidates it. The renderer never maps the persistent store.

Page controls intentionally have split semantic and native ownership. A renderer cannot transmit an
`HWND`, callback pointer, brush, font handle, or other GDI/USER object. It sends a `ControlId`, kind,
state, and finite rectangle. The browser creates and destroys native resources, applies quotas, and
returns normalized input. A future owned-widget painter can remove the native half without changing
DOM ownership.

## Process allocation

`BrowsingContextId` is stable for the life of a top-level context and survives renderer replacement.
Initially each top-level context receives a distinct renderer, and its descendant frames share that
renderer. The browser owns a `ProcessPolicy` interface that maps a frame/navigation to a renderer;
the protocol never assumes that a frame and its parent share a process.

A future policy may allocate by site, origin-keyed agent cluster, or browsing context group. Moving a
frame then means creating a new renderer-side document endpoint and updating browser routing; it does
not change `FrameId`, history identity, cookie ownership, or Fetch ownership. A renderer OS process
ID is diagnostic data only and is never an authority token.

## IPC contract

### Transport and launch bootstrap

The first Windows transport uses two anonymous pipes, one per direction. The browser creates them
and passes only the renderer ends through `PROC_THREAD_ATTRIBUTE_HANDLE_LIST`; all other handles are
non-inheritable. The child reads and writes its standard handles, so no raw handle value appears in
the command line or protocol. Dedicated broker threads perform blocking pipe I/O and deliver decoded,
bounded messages to the UI thread; renderer I/O can never block the Win32 message pump.

`CreateProcessW` receives an explicit executable path and `STARTUPINFOEX`. The child starts with
`CREATE_NO_WINDOW` and no visible UI. The command line selects renderer mode and contains a
single-use random bootstrap nonce, but no URL, cookie, storage value, pointer, or handle. The first
valid frame must echo that nonce and negotiate the protocol before any document data is sent.

Named pipes or shared memory require a separate decision. In particular, default named-pipe ACLs are
not acceptable, and this protocol never sends raw mapping handles as integers. The initial copy cost
is preferred over an unaudited zero-copy path.

### Framing

Each frame starts with this fixed 32-byte, little-endian header:

```text
magic[4] | major:u16 | minor:u16 | kind:u16 | flags:u16 |
payload_length:u32 | renderer_session:u64 | sequence:u64
```

- `magic` is `BRZ1`. A bad magic, reserved flag, non-monotonic sequence, or invalid length closes the
  renderer session.
- `major` changes for incompatible schemas. `minor` may add explicitly optional fields. The browser
  and renderer negotiate one version in `Hello`/`Ready`; an unknown required message is fatal.
- `renderer_session` is allocated by the browser and never reused during the browser process. It
  rejects buffered data from a prior renderer incarnation.
- Each direction has an independent, strictly increasing sequence starting at one. Exhaustion is a
  fatal protocol error rather than wraparound.
- The decoder reads the complete bounded payload before interpreting fields. It never allocates from
  an unchecked length or count and never recursively decodes an unbounded structure.

Payload schemas use explicit integer, boolean, finite floating-point, bounded UTF-8, byte-string,
list, and tagged-enum fields. Production Stage 1 will implement a field-by-field codec with property
and malformed-input tests; it will not deserialize arbitrary Rust object graphs. Bulk bytes are sent
as bounded chunks and are never IPC-compressed.

IPC payloads contain values and typed logical IDs only. They never contain pointers, Rust references,
Windows process/thread IDs used as authority, file descriptors, `HANDLE`, `HWND`, GDI objects, or
other process-local tokens.

### Identifiers

All scalar IDs reserve zero as invalid. Browser-issued counters use checked increments and are never
reused during the browser lifetime. Renderer-issued node values are scoped by both session and
document, so a compromised renderer cannot alias another context.

| Identifier | Allocator | Lifetime and scope |
|---|---|---|
| `RendererSessionId(u64)` | Browser | One child-process incarnation; present in every frame header |
| `BrowsingContextId(u64)` | Browser | One top-level context; survives navigation and renderer crashes |
| `FrameId(u64)` | Browser | One navigable/frame until removal; not tied to a process |
| `NavigationId(u64)` | Browser | One attempted navigation; cancellation and commit are terminal states |
| `DocumentId(u64)` | Browser | One committed document; same-document navigation retains it |
| `RequestId(u64)` | Browser | One accepted network operation until completion or cancellation |
| `RequestToken(u64)` | Renderer | Correlates a proposed fetch until the browser accepts it and returns a `RequestId` |
| `NodeId(u128)` | Renderer DOM | Existing allocation namespace plus local sequence; always paired with `DocumentId` on IPC |
| `ControlId(u64)` | Renderer | One semantic page control inside one `DocumentId` |
| `DisplayListRevision(u64)` | Renderer | Monotonic within one document; older updates are discarded |

The browser derives a document's URL and origin from its accepted navigation response. A renderer
cannot choose those fields by placing a different origin in a message. Similarly, browser code
constructs `FetchRequest` from the active document plus a whitelisted intent; internal cancellation
signals, credential policy, cookie headers, and response limits are not renderer-controlled fields.

### Message families

The names below define logical typed variants, not stringly dispatched commands. Stage 1 may split
large families into focused modules, but it must preserve their authority and state transitions.

| Family | Browser to renderer | Renderer to browser |
|---|---|---|
| Session | `Hello`, `Ping`, `Shutdown`, `ProtocolFailure` | `Ready`, `Pong`, `ShutdownComplete`, `RendererDiagnostic` |
| Context | `AttachContext`, `DetachContext`, `SetVisibility`, `SetViewport`, `SetPreferences` | `ContextAttached`, `PreferredSizeChanged` |
| Navigation | `BeginDocument`, `DocumentBodyChunk`, `EndDocument`, `CancelNavigation`, `NavigationRejected` | `RequestNavigation`, `DocumentReady`, `SameDocumentNavigation`, `NavigationFailed` |
| Fetch | `FetchAccepted`, `ResponseHead`, `ResponseBodyChunk`, `ResponseEnd`, `FetchFailed` | `StartFetch`, `CancelFetch` |
| Storage | `CookieSnapshot`, `CookieChanged`, `StorageResult`, `StorageInvalidated` | `SetCookie`, `StorageRead`, `StorageWrite`, `StorageDelete`, `StorageClear` |
| Permissions | `PermissionDecision`, `PermissionRevoked` | `RequestPermission` |
| Input | `PointerInput`, `KeyboardInput`, `TextInput`, `FocusInput`, `ScrollInput` | Navigation/default-action results are explicit typed messages |
| Presentation | `PresentationAcknowledged` | Length-declared immutable presentation revisions with controls, title, status, and diagnostics |
| Lifecycle | `LifecycleInput`, `CancelDocument`, `Shutdown` | `RuntimeUpdate`, `DocumentFailed`, `ShutdownComplete` |

There is deliberately no generic “invoke browser API”, “execute script”, “set arbitrary header”, or
“paint native handle” message. New browser authority requires a named message, validation rules, a
budget, tests, and review of this ownership table.

`RuntimeUpdate` carries bounded nonvisual progress such as console output, storage changes,
navigation requests, load metrics, and the next timer deadline. Only a visual invalidation emits a
presentation. The browser installs a presentation transactionally, preserves unchanged image and
native-control state, and invalidates only the reported display-list damage. This separation keeps
timer-heavy pages from repeatedly serializing and repainting an unchanged document.

## Initial limits

These are implementation ceilings, not web-standard limits. The browser sends negotiated frame
limits in `Hello`; a renderer may operate below them but cannot raise them. The source of truth for
the process, protocol, state, and parser budgets is `src/limits.rs`. Changes require regression and
compatibility evidence.

| Resource | Initial ceiling | Enforcement |
|---|---:|---|
| IPC control payload | 256 KiB | Reject frame before payload allocation |
| IPC payload, any kind | 8 MiB | Close session on a larger declared length |
| Navigation/presentation transfer chunk | 1 MiB | Sender chunks; receiver validates offset and declared total |
| Fetch response chunk | 64 KiB | Producer and consumer reject a larger stream chunk |
| Queued browser commands | 8 | UI uses nonblocking enqueue; reject overflow |
| Queued decoded renderer frames | 8 | Pipe reader backpressures until the broker drains |
| Deferred messages during a renderer Fetch wait | 64 | Reject the renderer session |
| Queued renderer UI events | 256 total; at most 2 presentations | Overflow terminates only that renderer session |
| Queued Fetch response chunks | 8 | Full queue backpressures the network worker |
| URL field | 16 KiB UTF-8 | Reject request or navigation intent |
| Serialized HTTP headers | 256 entries; 1 KiB names; 16 KiB values | Fetch failure; renderer cannot override browser-only headers |
| Renderer Fetch requests | 256 per batch; at most 8 transport workers concurrently | Reject excess requests before WinHTTP |
| Individual request/response body | 16 MiB | Reject or abort the typed stream |
| Aggregate renderer request bodies | 32 MiB per batch | Reject the batch |
| Aggregate page resources | 32 MiB per document | Stop admitting additional resources |
| Immutable presentation archive | 64 MiB | Reject update and retain the last valid revision |
| Decoded raster asset | 32 million pixels / 128 MiB BGRA | Decode in renderer; transfer in chunks |
| Script-visible cookie snapshot / assignment | 64 KiB / 4 KiB | Reject state message |
| Cookies | 3,000 total; 180 per domain | Deterministic oldest-cookie eviction |
| Web Storage | 5 MiB and 1,024 entries per origin | Reject mutation with quota error |
| Renderer committed memory | 1 GiB hard process and Job limit | Terminate only that renderer |
| Renderer child processes | Zero | Child-process policy; Job active-process limit is one |
| Startup handshake | 5 seconds | Terminate child and report launch failure |
| Cooperative shutdown | 2 seconds | Close/terminate the renderer Job after grace period |
| Responsiveness | Ping every 1 second; unresponsive at 3 seconds after first paint | Keep browser interactive; terminate after 1 second of grace and reload a presented document once |
| Hidden-test hang | 30 seconds | Terminate renderer and fail the test deterministically |
| Crash loop | 3 crashes for one context in 60 seconds | Suppress automatic process recreation |

There is no fixed renderer-lifetime CPU quota: a long-lived page can legitimately accumulate CPU.
The browser records Job accounting and detects missing event-loop progress. Per-task CPU and
recursion budgets belong to issue #10 and must interrupt or tear down the document without blocking
the browser UI.

## Required flows

### Top-level navigation

1. Browser normalizes input, checks policy, allocates `NavigationId`, updates pending UI state, and
   starts its document `FetchController`.
2. Browser Fetch performs redirects, TLS, cookie, credentials, and response-policy work. The
   renderer never sees ambient credentials or a mutable cookie jar.
3. After accepting a displayable response, the browser allocates `DocumentId`, derives URL/origin,
   and transfers the bounded body with `BeginDocument` plus length-declared chunks. Navigation is
   currently buffered before this transfer; subresource Fetch responses are network-progressive.
4. Renderer creates the document realm, parses, requests subresources, runs eligible scripts, and
   returns `DocumentReady` plus a display-list revision.
5. Browser validates that session, context, frame, navigation, and document are still active before
   committing history and presenting. Late messages are discarded.

### Renderer-initiated Fetch

1. Renderer sends `StartFetch` with `DocumentId`, `RequestToken`, method, URL/reference, destination,
   mode intent, allowed script headers, and optional bounded body.
2. Browser validates the active document and origin, constructs a new `FetchRequest`, applies header
   guards and credentials policy, allocates `RequestId`, and replies with `FetchAccepted`.
3. Browser streams the filtered response or a typed Fetch failure. Backpressure pauses delivery; it
   never grows an unbounded queue.
4. Navigation, document discard, renderer cancellation, or process exit aborts the browser-owned
   request through the document `FetchController`.

### Cookies and storage

At document creation the browser provides the non-`HttpOnly` cookie view and permitted storage
snapshot needed by synchronous APIs. Renderer mutations are requests, not durable writes. The
browser applies URL/origin and attribute rules, updates persistent state, then returns a corrected
versioned snapshot to the requesting active document. An outdated cache version cannot overwrite a
newer browser value. Cross-document `storage` events remain follow-up compatibility work.

### Presentation and controls

The renderer sends immutable display-list revisions containing finite geometry, colors, text, and
logical asset IDs. The browser checks every count, index, rectangle, byte range, and revision before
painting. Rejection retains the last valid surface and records a bounded diagnostic; it never follows
renderer pointers.

Remote image, SVG, and webfont bytes are decoded in the renderer. Text is shaped and rasterized
there by the Rust stack accepted in ADR 0002. The browser receives bounded pixel/drawing assets and
glyph placements, validates them, and composites them without receiving remote font bytes or
parsing font tables.

Renderer control descriptions create browser-owned native controls. Browser input is normalized and
tagged with `DocumentId`, a renderer-issued `NodeId` where needed, and an event sequence. Focus or input targeting a stale control
is dropped before DOM dispatch. Content coordinates are renderer-hit-tested against the current
layout; the browser never chooses a page link target. Text controls send a bounded full value plus
UTF-16 selection offsets, while native control activation round-trips only the renderer-issued
`NodeId`. Presentation acknowledgements distinguish an installed/presented revision from a rejected
one and report whether its native controls were applied.

Ordered keyboard, text, focus, and lifecycle messages are never dropped. Pointer moves may be
dropped at browser-command backpressure, the browser retains the latest unsent scroll for retry,
and obsolete scroll or pointer-move updates are coalesced within the current continuous-input run
while the renderer waits for a browser-brokered Fetch response. A click, key, focus, or lifecycle
input is an ordering barrier that coalescing never crosses. This prevents alternating high-rate
state updates from crowding the bounded deferred queue without reordering discrete events.

## Crash, timeout, cancellation, and teardown

### Crash or pipe failure

The browser treats process exit, broken pipe, invalid framing, and fatal decode errors identically:

1. Atomically mark `RendererSessionId` dead and stop accepting its messages.
2. Abort every document Fetch controller and remove outstanding request mappings.
3. Destroy browser-owned page controls and discard unpresented display-list revisions.
4. Close IPC and the Job handle, retaining exit code, Job accounting, active URL, and bounded
   diagnostics for the task manager.
5. Present a browser-owned crash surface. A task-budget exit after a successful presentation may
   refetch that same history entry once; other reloads remain user initiated, and form submissions
   and script navigations are never replayed automatically.

### Unresponsive renderer

The broker's process monitor is independent from both the renderer event loop and the UI thread.
Browser-to-renderer `WriteFile` calls run on a dedicated bounded writer thread; otherwise a child
that stops reading its command pipe can block the broker that must enforce its deadline. Missing
progress for three seconds after first paint marks the page unresponsive; a further one-second
grace period terminates the renderer Job, while chrome, history, and sibling tabs remain available.
First presentation retains its separate 25-second ceiling. Hidden automation also has an outer
deterministic deadline so CI cannot hang indefinitely.

### Navigation cancellation

A replacement navigation immediately invalidates the old `NavigationId` and aborts browser-owned
network work. `CancelNavigation`/`DiscardDocument` requests cooperative renderer cleanup. The browser
does not wait for acknowledgement before loading another renderer or updating pending chrome; if a
discarded document does not acknowledge within the two-second shutdown grace period, its renderer is
terminated when policy permits.

### Normal shutdown

Browser sends `Shutdown`, stops new broker requests, aborts Fetch, and waits at most two seconds for
`ShutdownComplete` and process exit. It then closes the Job handle with kill-on-close enabled. A
renderer cannot keep Breeze alive, orphan a descendant, or retain a persistent-state handle.

## Windows containment policy

Before processing remote content, a renderer launch must apply all required controls or fail closed:

- Create an AppContainer profile and launch with `PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES` and
  no network, private-network, broad-file, registry, or device capabilities.
- Create a Job Object before launch, set `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`,
  `JOB_OBJECT_LIMIT_ACTIVE_PROCESS` to one, and the committed-memory ceiling, and attach it at process
  creation through `PROC_THREAD_ATTRIBUTE_JOB_LIST`. Do not enable breakaway.
- Set `PROC_THREAD_ATTRIBUTE_CHILD_PROCESS_POLICY` to
  `PROCESS_CREATION_CHILD_PROCESS_RESTRICTED` even though the Job also constrains descendants.
- Use `PROC_THREAD_ATTRIBUTE_HANDLE_LIST` so only the two renderer pipe ends are inherited.
- Build an explicit, case-insensitively sorted environment block from the audited Windows bootstrap
  allowlist plus drive-current-directory entries; reject every non-allowlisted child variable.
- Apply compatible creation-time mitigations for DEP, ASLR, strict handle checks, extension-point
  disabling, CFG, image loading, and dynamic code. Each policy needs a launch test on supported
  Windows versions; unsupported optional hardening is reported, while failure of AppContainer, Job,
  child-process, or handle-isolation setup is fatal.
- The renderer's GDI/font text dependency has been removed. Keep the Win32k system-call ban
  disabled until remaining renderer platform calls have an explicit compatibility launch test; do
  not silently advertise that mitigation before its test passes.

The browser monitors Job notifications and the process handle for accounting and termination. Job
limits complement AppContainer; Microsoft documents that Job security limits do not replace
per-process security policy.

## Migration plan

Each stage must preserve behavior and performance baselines, remain headless in automation, and
avoid a mixed state that is described as secure before its boundary is enforced.

### Stage 0: Record the invariant (this ADR)

- Freeze ownership, IDs, message families, limits, and failure semantics.
- Keep the current application single-process.
- Make new features follow the target ownership even while calls are still in-process.

Completed.

### Stage 1: Introduce an in-process protocol seam

- Add focused `renderer_protocol` ID, message, limits, codec, and state-machine modules with malformed
  input, round-trip, version, sequence, and budget tests.
- Split privileged context state from the document realm behind `BrowserEndpoint` and
  `RendererEndpoint` traits.
- Replace renderer-facing heap pointers and direct `BrowserState` access with owned typed messages.
  Private worker-to-UI messages may remain internal but cannot become the process protocol.
- Benchmark message construction and display-list serialization before choosing further bulk
  transport optimizations.

Completed. The checked codec uses fixed framing, typed IDs, monotonic sequences, explicit versions,
bounded allocations, and owned payloads.

### Stage 2: Process-launch and crash-recovery spike ([#33](https://github.com/VictorZakharov/better-web-browser/issues/33))

- Add a hidden `--renderer-process` mode to the existing executable.
- Launch a capability-free AppContainer child with explicit inherited pipes, Job limits, child
  restriction, compatible mitigations, and an explicit environment allowlist. The initial spike
  handled only lifecycle and test messages before later stages admitted remote bytes.
- Add hidden integration tests for handshake, clean exit, startup timeout, malformed frame, forced
  abort, Job kill-on-browser-close, child-launch denial, direct-network denial, and recovery to a
  browser-owned crash surface.
- Record renderer PID, exit reason, memory, CPU, and restart count in task-manager diagnostics.

Completed on 2026-08-14 and subsequently hardened. Dedicated reader and writer threads own blocking
pipe I/O; the nonblocking broker owns heartbeats, process accounting, shutdown deadlines, and Job
termination. The UI thread consumes typed events; Task Manager presents the browser and per-tab
renderers as a process tree. Abnormal exit preserves the browser and sibling tabs, and Reload starts
a replacement process with fresh session and document identities.

### Stage 3: Broker navigation, Fetch, cookies, and storage

- Keep `fetch` and `winhttp` in the browser and translate renderer intents into browser-built
  requests.
- Stream navigation and subresource bodies over bounded IPC with cancellation and backpressure.
- Move cookie and storage authority out of `ScriptRuntime`; expose versioned document snapshots.
- Run loopback-only integration tests proving that a renderer cannot bypass origin, credential,
  CORS, redirect, cookie, or response-body policy.

Completed on 2026-08-23. Each renderer handshake is bound to a browser-issued
`BrowsingContextId`. Browser-owned cookies, persistent `localStorage`, tab-scoped
`sessionStorage`, versioned projections, guarded request reconstruction, response filtering,
bounded command/event queues, progressive subresource response IPC, backpressure, cancellation,
and stale-document rejection are implemented and tested. Navigation is still buffered before its
bounded IPC transfer, and the JavaScript `Response` body is still assembled in the renderer. The
full decision and remaining boundaries are recorded in
[ADR 0004](0004-browser-state-and-fetch-broker.md).

### Stage 4: Move the document engine and decoders

- Move HTML/CSS parsing, DOM, Boa, scheduler, style/layout, Reader extraction, and remote decoders to
  the renderer endpoint.
- Replace the temporary GDI text-measurement bridge and keep webfont bytes out of the browser.
- Serialize display-list and semantic-control updates; validate them before browser presentation.
- Preserve the full rebuild fallback while incremental invalidation from issue #8 is adopted.

Remote-document decoding, HTML/CSS parsing, DOM, Boa, scheduler, style/layout, Reader extraction,
image/SVG/webfont decoding, dedicated Workers, text shaping/rasterization, and immutable
presentation construction now run in the renderer. The privileged in-process page-engine fallback
and temporary GDI text-measurement bridge have been removed. The browser only validates and
composites renderer-owned glyph raster output as specified by ADR 0002.

The trusted local home page follows the same document-start IPC path rather than constructing a
browser-process `Page`. Opt-in benchmark selector diagnostics are sent as a bounded selector list
on `DocumentStart`, evaluated beside the renderer-owned DOM and computed styles, and returned as a
size-bounded JSON value inside the validated presentation archive. The browser never retains a DOM
or CSS style set for diagnostic queries. Reader mode is the narrower exception: extraction happens
in the renderer, while the browser lays out only the bounded semantic `Document` projection with
trusted local UI fonts.

### Stage 5: Make one renderer per top-level context the default

- Route native input, viewport, focus, lifecycle, and presentation through IPC.
- Enable crash/error surfaces, reload, cancellation, and task-manager controls in normal builds.
- Remove the old in-process document execution path after hidden browser, WPT, visual, performance,
  hostile-input, and crash-recovery suites pass.

Completed on 2026-08-23. One renderer per live tab is the only top-level document execution path.
Typed, bounded IPC now carries viewport, pointer, keyboard, full native-control text values with
selection, focus, scroll, visibility/freeze lifecycle, navigation disposition/cause, and explicit
presentation acknowledgements. The renderer owns content hit testing, DOM event dispatch, control
state, and GET-form/link default actions; the browser owns only native-event capture, HWND
projection, validation, final composition, history policy, and privileged navigation/Fetch.

Crash/error surfaces, cancellation, reload with fresh identities, heartbeat hang termination, and
Task Manager **End process** are active in normal builds. Hidden real-process tests cover trusted
input and lifecycle dispatch, stale document/sequence rejection, presentation acknowledgement,
user-navigation disposition, crash/hang containment, explicit termination, sibling survival, and
replacement renderer recovery. IME/composition, cancelable `beforeinput`, and broader form behavior
remain web-compatibility work, not alternate document execution paths.

### Stage 6: Evolve allocation policy

- Add cross-origin frame routing and an out-of-process frame presentation model.
- Evaluate site, origin-keyed agent-cluster, and browsing-context-group allocation against HTML and
  security requirements.
- Treat full site isolation and shared-memory/GPU transport as separate reviewed decisions.

## Current containment acceptance

The production renderer boundary is accepted only while these executable invariants remain true:

- every automated launch is hidden and uses `CREATE_NO_WINDOW`;
- the browser supplies an explicit executable path, an allowlist of exactly two inherited handles,
  and an audited allowlist-only environment block;
- a successful nonce/version handshake completes within five seconds;
- AppContainer has no network capability, direct loopback/Internet attempts fail, and broker IPC
  still works;
- the renderer cannot create a child process or outlive the browser Job;
- renderer abort, access violation, OOM termination, and native stack overflow each leave browser
  chrome and a sibling renderer responsive, record the exit, and show a browser-owned recoverable
  error surface;
- reload after a fatal exit creates a fresh process ID, renderer session, and document identity;
- malformed and oversized frames terminate only the renderer session;
- clean shutdown and timeout paths leak no process or pipe handles; and
- renderer-issued nodes receive pointer/keyboard/text/focus events only for the active document and
  monotonic input sequence, while stale input cannot mutate a replacement document;
- Task Manager termination stops the selected real renderer and a user reload creates a fresh
  renderer session; and
- all tests run without visible browser or console windows.

## Consequences

### Benefits

- Remote parser and script failures stop being browser-UI failures.
- Networking and persistent state have one policy owner, reducing confused-deputy behavior.
- Stable IDs and versioned messages make stale work and crash recovery explicit.
- Process placement can evolve without rewriting Fetch, DOM identity, or navigation history.
- Task-manager accounting can report a real renderer boundary instead of only worker activity.

### Costs and risks

- Body, decoded-asset, and display-list copies add latency and memory pressure until measurement
  justifies a separately reviewed bulk transport.
- Synchronous APIs such as `document.cookie` require carefully versioned caches rather than direct
  browser calls.
- Native controls and the browser's GDI pixel compositor remain Windows-specific presentation
  constraints, but neither parses remote fonts nor measures document text.
- Protocol validation, cancellation races, process launch, AppContainer compatibility, and crash
  recovery substantially increase test surface.
- One renderer per top-level context isolates contexts from each other but does not isolate mutually
  hostile origins inside the same context.

## Primary references

- [WHATWG HTML: navigables and browsing contexts](https://html.spec.whatwg.org/multipage/document-sequences.html)
- [WHATWG HTML: event loops](https://html.spec.whatwg.org/multipage/webappapis.html#event-loops)
- [WHATWG DOM Standard](https://dom.spec.whatwg.org/)
- [W3C UI Events](https://www.w3.org/TR/uievents/)
- [CSSOM View: scrolling events and state](https://drafts.csswg.org/cssom-view/)
- [WHATWG HTML: interaction, focus, and page visibility](https://html.spec.whatwg.org/multipage/interaction.html)
- [WHATWG Fetch Standard](https://fetch.spec.whatwg.org/)
- [RFC 10025: HTTP State Management Mechanism](https://www.rfc-editor.org/rfc/rfc10025.html)
- [Microsoft: Launch an AppContainer](https://learn.microsoft.com/windows/win32/secauthz/implementing-an-appcontainer)
- [Microsoft: Job Objects](https://learn.microsoft.com/windows/win32/procthread/job-objects)
- [Microsoft: `UpdateProcThreadAttribute`](https://learn.microsoft.com/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute)
- [Microsoft: `CreateProcessW`](https://learn.microsoft.com/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessw)
- [Microsoft: Process mitigation policies](https://learn.microsoft.com/windows/win32/api/processthreadsapi/nf-processthreadsapi-setprocessmitigationpolicy)
- [Microsoft: Pipe handle inheritance](https://learn.microsoft.com/windows/win32/ipc/pipe-handle-inheritance)
