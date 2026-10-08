# Numeric-array union conversion

WebGL1 and WebGL2 use genuine `Float32Array`/`Int32Array`/`Uint32Array`
brands for their corresponding numeric-list union members. A matching view
does not consult an author iterator, `Symbol.toStringTag`, shadowed lengths,
or buffer properties. A different typed-array kind or a proxy instead goes
through ordinary sequence conversion, including its observable iterator and
element conversion order.

The binding captures intrinsic getters and constructors once per realm. It
snapshots matching typed data during argument conversion before later offset
or length getters can change the backing store. This follows the existing
multi-draw ownership policy and the matched Chrome 154 reference: mutating or
detaching the original array in an offset getter leaves the prior conversion
snapshot intact. No original/shared buffer span is handed to Rust or the GL
owner thread. Shared snapshots are not claimed to be transactionally atomic
against another agent writing individual elements.

`[AllowShared]` permits fixed-length shared storage; it does not imply
`[AllowResizable]`. Matching resizable/growable views throw `TypeError` during
IDL conversion, including after context loss or for a null uniform location.
A view already detached at conversion instead queues WebGL `INVALID_VALUE`
without invoking the driver, even with a null uniform location. Context loss
suppresses that native-validation error but still performs IDL conversion.

The source snapshot has a one-million-element resource cap for WebGL2, while
the submitted command slice remains bounded at 8,192 elements. WebGL1 retains
its prior 8,192-element conversion cap. These are explicit implementation
resource budgets, not advertised standards limits or HTML5test behavior.

The shared `tests/webgl/numeric-unions.js` regression runs in real Window and
Worker contexts and against unmodified hidden Chrome. The reference checks 67
assertions, including real uniform/attribute/matrix readback, subclass brands,
sequence fallbacks, detached-buffer errors, resizable storage, loss and offset
side effects, unsigned integer attributes and a clear operation with real pixel
readback. Fixed shared/growable storage is additionally checked when that
realm actually exposes `SharedArrayBuffer`; the ordinary non-isolated Chrome
reference does not expose it and is not counted as a shared-memory comparison.

Primary behavior references:

- [WebGL1 numeric typedefs](https://registry.khronos.org/webgl/specs/latest/1.0/#TYPES)
- [WebGL2 API](https://registry.khronos.org/webgl/specs/latest/2.0/)
- [Web IDL union conversion](https://webidl.spec.whatwg.org/#es-union)
- [Web IDL buffer types and AllowResizable](https://webidl.spec.whatwg.org/#es-buffer-source-types)
