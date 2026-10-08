# WebGL upload and readback ownership

## Public memory remains independent

WebGL uploads consume the bytes selected by the author's BufferSource view and
WebGL2 element-offset overload. They do not transfer, detach or retain a borrowed
pointer to that author's ArrayBuffer. Mutation or an explicit structured-clone
transfer after the synchronous call cannot change previously uploaded GPU data.
Readback updates only the admitted destination range; surrounding bytes, row
padding and skipped pixels retain their original values.

WebGL's `[AllowShared]` buffer arguments do not imply `[AllowResizable]`.
Both in-bounds and out-of-bounds views of resizable/growable backing stores
reject with `TypeError` before native submission. A separately made fixed-length
copy is admissible and remains independent when the original is resized.
Captured backing-store getters enforce the same rule for upload, readback and
numeric-list routes, including argument conversion on a lost context. See
[Web IDL buffer admission](https://webidl.spec.whatwg.org/#AllowResizable).

These contracts follow the [WebGL buffer API](https://registry.khronos.org/webgl/specs/latest/1.0/#5.14.5)
and [WebGL2 buffer and pixel overloads](https://registry.khronos.org/webgl/specs/latest/2.0/).
The optimization is internal ownership transfer, **not** a public zero-copy API.

## The native copy has one owner

V8's value conversion already produces an independent Rust byte allocation.
The Window/Worker bridge can move that allocation into the realm's WebGL
registry and the existing bounded single-owner request channel. It never borrows
V8 backing storage across a host call. Unsupported platforms keep declining
native context creation; this path does not fabricate GPU functionality.

The same registry checks host liveness and realm ownership, flushes preceding
commands, validates command/upload limits, invalidates affected synchronous
reply caches and handles lost contexts. Only the two established host operations
(`webglCommand` and `webglReadPixels`) select this routing. The binary-readback
operation still rejects every native operation except `readPixels` and
`getBufferSubData`.

For `bufferData`, the GL owner validates binding, transform-feedback conflicts,
usage, byte extent and storage admission before submission. After actual native
success and resource-ledger commit, the independent allocation becomes the
browser's CPU mirror. A failed request leaves the previous native storage and
mirror unchanged. Numeric-size allocation remains robustly zero-initialized.
CPU and GPU buffer copies are both charged by the existing high-water ledger;
no storage cap increases. Internal vectors with excess spare capacity are
normalized rather than retaining an unaccounted allocation.

Other upload operations borrow that independent allocation only during the
native command. `bufferSubData` still updates the existing mirror, and texture
uploads still obey their type/layout and native admission rules. No CPU mirror
or borrowed pointer aliases author memory after return.

For `readPixels`, the independent copy of the destination may become the native
output allocation. It is truncated only to the validated packed extent and
keeps original padding and out-of-bounds destination bytes. The returned owned
byte reply is then copied into the selected public view by the existing binding.
An invalid destination, framebuffer or pixel pair publishes no successful reply.
The bounded readback cache retains its own independent copy when eligible.

## Linked reflection is not mutable draw state

The attribute cache retains at most 32 linked-program records, each a fixed
32-bit input-slot mask keyed by browser program owner and link generation.
The mask comes from real native reflection, including every matrix column and
attribute-array slot. Relinking invalidates the prior mask before the new link
is queued or executed; binding a location alone does not change a linked input.

Every draw still validates current enabled arrays, live buffer objects, storage
capacity, stride, offset, instance count and divisor. Switching VAOs or shrinking
a buffer cannot reuse old bounds. Inactive enabled arrays still require a live
buffer, while only linked active inputs require an in-range vertex extent.
The native driver continues type, framebuffer, transform-feedback and program
validation. This cache does not store an earlier draw's validation outcome.

The WebGL range-checking contract permits either rejection of an out-of-range
vertex fetch or robust zero/in-buffer values. Breeze retains its explicit
`INVALID_OPERATION` policy; Chrome 154's controlled fixture uses robust fetches
without an error. Shared reference fixtures accept both permitted outcomes and
check valid-draw pixels and rejection atomicity where applicable, while native Breeze tests require its stricter
rejection. Out-of-bounds index-buffer fetches are different: those must error.

## Regression evidence

Real native tests assert allocation adoption, byte/pixel values, borrowed-source
independence, numeric zeroing, transaction rollback and capacity normalization.
The same public binding tests run in both Window and Worker, exercising source
mutation/transfer, typed element ranges, texture copies, destination subviews,
packed padding and failed reads. Dead-host tests cover both owned routing paths.

The public `owned-transfer-contract` fixture runs the same native integer/HDR,
DataView, resize-rejection and GPU-written pixel-buffer assertions in Window and
a dedicated HTTP-script Worker. Chrome 154 passes all six checks in each realm.
The fixture does not establish Blob-URL Worker support: the first hidden Breeze
run rejected that script URL scheme, so ordinary HTTP script loading is used
explicitly rather than presenting a Blob Worker as working.

WebGL1 readback also uses intrinsic destination branding and byte ranges. Author
properties named `buffer`, `byteOffset` or `byteLength`, and replacement public
typed-array constructors, cannot redirect a successful reply to another buffer.
The same regression runs on real WebGL1 and WebGL2 providers in both realms.

Native attribute tests cover repeated actual draws and pixels, changed buffers,
stride/offset, VAO switches, matrix columns, instanced divisors and relinking.
The owned `attribute-draw-throughput` and `buffer-upload-throughput` fixtures
finish native work and assert pixels/bytes; their submission times alone are not
GPU completion times or unchanged-game acceptance.

## Final-source completed-work comparison

Three fresh alternating runs compare merged #229, the complete October 8 source
release, and Chrome 154. All use scale 1, a 1262×539 viewport, disabled CPU
sampling, and no concurrent builds/tests. These are combined-batch observations,
not isolated attribution to ownership or a claim about whole-browser speed.

| Median, including completion readback | Merged #229 | October 8 release | Chrome 154 |
| --- | ---: | ---: | ---: |
| 2,000 draws with eight active attributes | 28.4 ms | 15.3 ms | 2.5 ms |
| 32 replacements of one 4-MiB buffer | 84.7 ms | 32.1 ms | 176.8 ms |

Every capture passes native pixel assertions; the upload fixture additionally
checks bytes and draws after mutating the author source. Draw observations are
27.9/28.4/29.6 ms before and 17.1/15.3/13.6 ms after. Upload observations are
84.7/85.3/81.4 ms before and 29.6/32.1/37.1 ms after. Chrome's corresponding
draw observations are 2.9/2.5/2.5 ms and uploads 170.7/220.2/176.8 ms. Its upload
cost in this bounded replacement/readback workload is not evidence that Breeze
is generally faster at GPU uploads or that the browsers use identical drivers.
