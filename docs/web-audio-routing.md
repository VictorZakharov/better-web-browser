# Web Audio channel routing and author processing

This batch extends the existing [offline and live graph](web-audio.md), not
the browser's codec claims. It implements actual channel buses, feedback
scheduling, and legacy author-produced PCM. HTML5test remains a discovery
guide, not an acceptance test for these behaviors.

## Channel configuration

`AudioNode` has inherited, branded `channelCount`, `channelCountMode`, and
`channelInterpretation` accessors. Constructors whose dictionaries inherit
`AudioNodeOptions` read those members before their own members. Buffer,
constant, and microphone source dictionaries do not inherit those options;
unknown members are ignored and cannot change a source's physical format.

| Count mode | Input bus width |
| --- | --- |
| `max` | Largest connected bus, or one silent channel |
| `clamped-max` | Smaller of that width and `channelCount` |
| `explicit` | Exactly `channelCount` |

The splitter keeps its explicit, discrete layout and one channel per
output. The merger has independent explicit mono inputs and preserves
silent port positions. Convolvers, compressors, and panners admit at most
two input channels and do not accept `max`. Offline destination width is
fixed; live logical destination width cannot exceed the stereo device.

One sparse mixing implementation handles speaker layouts 1, 2, 4, and 6.
It includes center-only mono-to-5.1 routing, surround contribution during
downmix, and omission of LFE when downmixing 5.1. Other layouts and
`discrete` interpretation copy matching channel indices and pad silence.
Mixing occurs at the node input, before gain, shaping, filtering, panning,
or convolution. Intermediate buses are not prematurely reduced to the
destination's width. Analyser FFT/time-domain analysis uses a mono speaker
downmix of its configured input signal; pass-through keeps that input bus.

AudioParam connections are individually downmixed to mono, then summed
with intrinsic automation. This follows the explicit `connect(AudioParam)`
rule in [Web Audio §1.5.5](https://www.w3.org/TR/webaudio-1.1/#dom-audionode-connect-destinationparam-output),
rather than inferring one surround layout from heterogeneous connections.

## Legal feedback and changing layouts

Connections may form cycles. The graph plan includes AudioParam owners
and the listener-to-panner dependency. It finds strongly connected
components, splits every cyclic DelayNode into a reader and writer, and
mutes components that still contain a cycle. Independent routes survive.
All readers run before writers, so feedback observes previous-quantum
history rather than advancing another delay or a shared source twice.
Plan invalidation follows successful topology, channel, and buffer edits.

An active cyclic delay has a minimum delay of one 128-frame quantum,
including when its supplied maximum delay is shorter. Delay history has
per-frame layout tags; channel changes reach the output when delayed
samples arrive. Fractional reads mix both historical layouts before
interpolation. IIR, biquad, and finite convolution tails retain a wider
bus while it contains pending non-silent history.

This is a bounded graph scheduler, not full active-processing/lifetime
conformance. It does not garbage-collect unreachable graph nodes during
a context's lifetime, or implement every inactive-cycle channel-retirement
rule. Oversampled WaveShaper lane retirement retains its existing policy;
it is not covered by the new delay/filter tail-width guarantee.

## Legacy ScriptProcessorNode

`createScriptProcessor(bufferSize, inputChannels, outputChannels)` creates
a real node for both contexts. This is the deprecated Web Audio API;
**AudioWorklet remains unavailable**. A requested size of zero selects
2048 frames. Sizes 256–16384, in powers of two, are supported. Each side
may have 0–32 channels, but both cannot be zero. The input count and
explicit count mode are fixed; interpretation may be speakers or discrete.

The renderer accumulates quantum input into a block and queues an
`AudioProcessingEvent` on the document's internal media-task queue. The
callback receives input and output AudioBuffers and the playback time of
the next output block. Initial output is silent for one block. Produced
output is acquired before callback microtasks; later edits cannot change
already produced PCM. A detached output channel is silent. Input-only
analysis and zero-input synthesis are supported, as is output to AudioParam.

Browser-generated processing events are trusted; author-constructed events
are not. Processing does not depend on the author's `dispatchEvent` or
timer overrides. Disconnected nodes do not dispatch. Reconnection starts
a fresh block after a disconnected render interval. Offline suspension
preserves partial input; live backpressure retains the same transport PCM.
Late callbacks cannot retroactively overwrite submitted audio. At most one
processing task is pending per node, and close/navigation retires its work.
Offline completion does not dispatch a final block whose output would
begin after the rendered buffer ends.

## Conversion, ownership, and bounds

Dictionary conversion reads each declared member once, in inherited then
lexical order. Integer arguments use Web IDL unsigned wrapping/truncation;
float arguments round before validation. BigInt and Symbol conversions
throw. Unknown enum strings in attribute setters are ignored; dictionary
enums reject them. Semantic validation follows completed conversion.

AudioBuffer construction, `createBuffer`, indexed access, and channel
copying share those conversion rules. Copy methods validate their receiver,
required arguments, and real non-shared, fixed-length Float32Array storage.
Detached views are valid empty inputs/outputs and do not mutate samples.
They use internal channel metadata, preserve unwritten ranges, and handle
overlapping views without relying on an author override of `getChannelData`.
Existing acquired-buffer snapshots remain bounded copies; this batch does
not implement the specification's complete ArrayBuffer detachment model.

Node-owned ScriptProcessor buffers reserve at most 16 MiB per context,
including accumulation, event copies, and acquired playback. Author-retained
event buffers are ordinary author allocations under the renderer VM limit.
The 256-node, 512-edge, 32 MiB delay-memory, per-quantum work, and 64-million
offline work bounds remain. Admission now accounts for wide intermediate
buses, indexed outputs, mixing fan-in, AudioParam fan-in, and retained DSP
history. Rejected connections and channel setters roll back graph state.
Runtime delay growth failures reject rendering and outstanding suspensions.

## Verification and reference differences

The original `benchmarks/alpha/fixtures/audio-routing.html` fixture checks
80 contracts with actual offline PCM and metadata, without browser-specific
branches, remote media, or an audible device. Unit tests execute exactly
the same assertion files. Additional tests cover all processing block
sizes, live PCM acknowledgements, suspensions, changing tails, and budgets.

The 2026-10-01 hidden release captures compared main `5986134` with this
batch, using fresh profiles and the same assertion files:

| Measurement | Before | After | Chrome 154.0.8037.92 |
| --- | --- | --- | --- |
| Original routing fixture | 34/80 | 80/80 | 72/80 |
| HTML5test rendered score, three runs | 487/588 each | 487/588 each | Not measured in this batch |
| HTML5test JavaScript errors / renderer exits | 0 / 0 | 0 / 0 | Not measured in this batch |

HTML5test used the default Breeze identity, 1280×720 window, 125% device
scale, `en-US`, and a 4.5-second settle. All six runs returned HTTP 200.
The owned fixture used a 4-second Breeze settle and a 2-second Chromium
settle with its explicit ready marker. Those are observation windows,
not a measurement of DSP performance. No HTML5test score gain is claimed.

Chromium reference differences are reported as failures, not hidden by
loosening the oracle or selecting assertions by browser:

| Reference difference | Failed contracts |
| --- | --- |
| Offline destination accepts a count edit where the generic channel constraints require `InvalidStateError` | 1 |
| Minimum cyclic delay, residual node/parameter muting, and disconnected feedback history | 4 |
| Produced legacy PCM does not begin at the event's reported playback time | 1 |
| Zero-input processing supplies null rather than a zero-channel event buffer | 1 |
| Callback microtask edits remain audible after the specified acquisition boundary | 1 |

The implementation follows the explicit specification contracts above;
this does not assert full browser parity or full Web Audio conformance.

Primary sources: [Web Audio 1.0 Recommendation](https://www.w3.org/TR/webaudio-1.0/),
[Web Audio 1.1 processing/channel model](https://www.w3.org/TR/webaudio-1.1/#rendering),
[legacy processing](https://www.w3.org/TR/webaudio-1.1/#ScriptProcessorNode),
and [Web IDL conversion](https://webidl.spec.whatwg.org/).
No dependency or copied third-party source was added: the matrices are
small normative equations, and callbacks reuse the existing task, event,
buffer, and PCM transport infrastructure.
