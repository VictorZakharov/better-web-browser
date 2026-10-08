# Windows V8 pointer-compression experiment

This is a local experiment, not a shipped backend change or a claim that the
unchanged Last Stand game now works. Breeze's normal pinned Windows archive,
Cargo dependency, source bindings and CI configuration remain unchanged.

## Pinned inputs and integration

The experiment uses rusty_v8 152.2.0 and its pinned V8 15.2.124.1 source commit
`c4ca1eccb90c5464d826b7713dc27178a53b0cfe`. Sources, restored pinned source-build
inputs, compiler downloads and outputs stay in ignored G: task directories.
Upstream V8/Rust bindings were not patched to make the build succeed. Existing
MIT/BSD/Unicode and preserved vendor licenses apply; no imported lines are
counted toward the hand-written batch target and no new binary is distributed.

The source build uses release mode, bundled libc++, pointer compression and its
shared cage, with the V8 sandbox and V8 checks disabled as in the existing
embedding. Intl, WebAssembly and Temporal remain enabled. Derived native
features include static roots, Sparkplug, Maglev and Turbofan. These defaults
make this a supported-configuration comparison, not an isolation of pointer
width from every dependent engine feature or from compiler differences.

The initial standalone library initialized and evaluated JavaScript but failed
to link into Breeze: V8's Windows standalone allocator shim defined
malloc/calloc/realloc/free alongside the executable's dynamic UCRT. The rebuild
uses upstream embedder options `use_allocator_shim=false` and
`use_partition_alloc_as_malloc=false`; it does not suppress linker failures.
V8's own internal PartitionAlloc remains enabled. The rebuilt library passed
initialization/evaluation and linked into the experimental browser.

The archive is 236,374,888 bytes, SHA256
`bccc693cfeb554a1482676b7139bbd7c991b13d943d0a9200740cf6e0ccec37b`.
Its matching generated binding SHA256 is
`9e2b10f7119a919f7336b9793f4e58412968c43a7d92033d5874d86fb67fb841`.
Never mix compressed and uncompressed bindings/libraries or reuse the normal
published-archive checksum for this source-built library.

## Matched game replay

The October 7 replay compares three fresh runs per browser, rotating Breeze
order, at the same 1262-by-539 CSS-pixel viewport and device scale 1, with twenty
seconds of settling. Both browsers are hidden; Chrome uses unified headless.
The same frozen game JavaScript/CSS/assets are served over loopback HTTP.
The snapshot still references live Google Fonts, so font-resource network
latency is not a byte-frozen part of the experiment.

These samples precede the subsequent clipped-shader extension. The tested
normal executable SHA256 is
`43994fb5a9da3988f92f005f243cc49f5e0ba0af84822240d09125fe90674654`;
the experimental executable SHA256 is
`3f6221908ad2f45a54b2838d6325415d6426c80d596eeaf325c579ee9c1a99a9`.

| Three fresh matched runs | Merged #228 | Normal batch build | Experimental V8 | Chrome 154 |
| --- | ---: | ---: | ---: | ---: |
| Median harness page-ready | 1,693.4 ms | 1,820.2 ms | 1,754.2 ms | 894.9 ms |
| Range | 1,599.4–1,834.6 ms | 1,731.1–2,838.1 ms | 1,743.3–1,794.2 ms | 886.8–986.2 ms |
| Captured loading screen remains | 3/3 | 3/3 | 3/3 | 0/3 |
| Execution-limit error | 3/3 | 3/3 | 3/3 | 0/3 |

All accepted navigation samples return HTTP 200. Chrome reaches the lobby;
Breeze remains at interrupted loading. The experimental build has document
timer promise-job failures in all three runs at the unchanged 2,000 ms limit.
Its small page-ready difference does not fix startup or justify distributing a
custom V8 backend. The normal build's median is slower than merged #228, and
its largest sample also has elevated resource-network time; no first-presentation
regression fix is claimed. Chrome's load observer and Breeze's renderer
presentation observer differ, so these values are not a browser-speed ratio.

Completed work and failure stages differ between samples, including worker
failures/main-thread fallback. Working-set and cumulative CPU totals therefore
do not establish a memory-efficiency or throughput ranking. A same-toolchain
noncompressed control was not pursued after the configuration failed the
startup acceptance criterion; no pointer-compression-only causal claim follows.

An initial file-URL attempt failed through the network-only navigation path.
An earlier comparison also lost its fixture server after a browser disconnected
during response transmission. Neither attempt contributes timing samples above.
The fixture server now has a forced-TCP-reset regression test that proves later
requests still succeed; the accepted comparison rejects failed navigation
immediately rather than treating connection-refused pages as faster runs.

Next investigation remains the actual document/worker computation and native
Canvas boundary, not a larger watchdog, smaller textures or modified game assets.
