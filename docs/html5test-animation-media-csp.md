# Image, editing, SVG, animation, media-track, and CSP standards slice

This batch extends browser behavior that applications can use, rather than
special-casing HTML5test. A feature row turning green is evidence of an API
being detected, not evidence that its entire specification is implemented.
The limits below are part of the compatibility contract for this technical
alpha. The [release status](../README.md) remains the broader support guide.

## Image decoding and editing

- A detached `Image` can complete a load, expose its intrinsic dimensions,
  resolve `decode()`, and become a Canvas 2D image source. Decode and canvas
  readback use the same decoded bitmap, not a synthetic placeholder. The
  existing same-origin/tainting rules still guard pixel access.
- `ClipboardEvent` and its `clipboardData` expose a separate, event-scoped data
  store. Constructing an event does not grant access to the OS clipboard.
- The supported `document.execCommand()` editing operations work on the
  active editable selection and report failure when the operation cannot be
  applied. The implementation does not treat a method's mere presence as a
  successful edit or insert content into unrelated elements.
- These contracts are exercised with decoded pixel and selection tests. The
  full legacy editing-command surface and OS clipboard permissions are not
  claimed.

References: [HTML image elements](https://html.spec.whatwg.org/multipage/embedded-content.html#the-img-element),
[Canvas image sources](https://html.spec.whatwg.org/multipage/canvas.html#image-sources-for-2d-rendering-contexts),
[Clipboard API and events](https://www.w3.org/TR/clipboard-apis/), and
[HTML editing](https://html.spec.whatwg.org/multipage/interaction.html#editing).

## SVG filter primitives

- Supported filter primitive geometry exposes live `SVGAnimatedLength`
  values. Updating a base length changes the primitive's observable geometry.
- The implemented color-matrix modes operate on raster pixels. Animated
  length reflection and actual filter output are tested independently.
- This is a bounded software subset, not a complete SVG filter graph or a
  claim that every SVG element or filter primitive is supported. Unsupported
  effects must not silently pretend to have modified pixels.

References: [SVG 2](https://www.w3.org/TR/SVG2/) and
[Filter Effects](https://www.w3.org/TR/filter-effects-1/).

## Web Animations

- `Element.animate()` creates a `KeyframeEffect` associated with an
  `AnimationTimeline`; `DocumentTimeline` time is used for playback and
  custom timeline origins. `AnimationEffect` and `AnimationTimeline` are
  present in the prototype chain. `id` and `timeline` options are honored.
- Animations contribute through the animation origin in the CSS cascade,
  leaving inline declarations untouched. Important author declarations
  remain higher priority. `getAnimations({subtree:true})` follows
  shadow-including ancestry; removed effects are excluded.
- Keyframe normalization retains specified versus computed offsets, merges
  property-indexed frames at their computed offsets, validates CSS values,
  and clones effects without shared mutable keyframes. Unsupported additive
  composition and pseudo-element targeting fail explicitly.
- Easing includes named, cubic Bézier, steps, and CSS `linear()` curves.
  Missing positions, plateaus, duplicate stops, discontinuities, and numeric
  overshoot are covered by tests. Out-of-range cubic Bézier input follows
  the endpoint tangents instead of being clamped away.
- Play and pause have asynchronous pending tasks and stable `ready` promise
  identity. A null timeline remains inactive until one is attached. A
  `finished` promise remains stable through completion and is replaced on
  replay. Finish, cancel, and remove dispatch playback events.
- Finished filling animations are removed only after later replaceable
  animations cover all of their target properties. `persist()` prevents that
  removal. `commitStyles()` requires a rendered target and writes effect
  values to the inline declaration block.
- The current painter interpolates numbers, colors, and supported 2D
  translations. Rotation, scale, additive/accumulative composition,
  pseudo-element effects, CSS-declared animation integration, and compositor
  offloading remain incomplete. Scripts should not assume full WAAPI support.

References: [Web Animations Level 1](https://www.w3.org/TR/web-animations-1/),
[CSS Easing Level 2](https://drafts.csswg.org/css-easing-2/), and
[CSS Cascade](https://www.w3.org/TR/css-cascade-5/).

## Media tracks

- Decoded media expose live audio and video track lists after metadata; the
  lists are empty before a resource is available. The decoder currently
  supplies one AAC audio stream and one H.264 video stream, so it does not
  fabricate a multi-track chooser. Audio-only metadata does not fabricate a
  video track.
- Disabling the audio track routes zero effective volume to the mixer while
  retaining the element's `volume` and `muted` settings. Deselecting the
  video track changes the renderer's displayed source while audio and the
  media clock continue. Source replacement resets selection and prevents
  old metadata tasks from re-adding stale tracks.
- Audio, video, text, and cue lists use live, read-only Web IDL indexed
  properties. Numeric property presence, enumeration, descriptors, and
  removal follow the current list contents. Track-list events carry the
  corresponding `TrackEvent.track` object.
- Multiple selectable decoded streams, arbitrary codecs, capture devices,
  and picture-in-picture are outside this slice. The track objects describe
  and control the streams the decoder actually accepted.

References: [HTML media tracks](https://html.spec.whatwg.org/multipage/media.html#media-elements),
[TrackEvent](https://html.spec.whatwg.org/multipage/media.html#the-trackevent-interface), and
[Web IDL legacy indexed properties](https://webidl.spec.whatwg.org/#legacy-platform-object).

## Content Security Policy reports

- Blocked inline and external script requests queue
  `SecurityPolicyViolationEvent` objects with directive, document/source URL,
  disposition, and line/column information when available. Cross-origin
  blocked source URLs are redacted, and sample text is included only when
  `report-sample` permits it.
- Script execution remains blocked by the existing CSP gate; reporting does
  not turn a denied script into an allowed one. This batch does not claim
  complete CSP directive coverage, report-only delivery, or Reporting API
  transport.

Reference: [CSP Level 3 violation events](https://www.w3.org/TR/CSP3/#violation-events).

## Verification

The focused Rust tests exercise actual DOM and script behavior: pixels,
editing, filter state/output, timing and cascade, promise/event ordering,
track changes, and CSP redaction. The hidden HTML5test run is a secondary
capability checkpoint. Neither a probe count nor a browser User-Agent mode
substitutes for these behavioral tests.
