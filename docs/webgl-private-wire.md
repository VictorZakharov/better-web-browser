# WebGL native command and reply ownership

The public [WebGL API](https://registry.khronos.org/webgl/specs/latest/1.0/)
performs its normal observable Web IDL conversion before constructing native
commands. Those command records, driver IDs and decoded native replies are
implementation-owned; they are not author objects or serialization callbacks.

The bridge previously used the page's replaceable `JSON.stringify`/`JSON.parse`
and ordinary inherited `toJSON` lookup on its command record and numeric lists.
It now captures the JSON functions at bootstrap and serializes a closed,
null-prototype command record with independently owned primitive lists.
The context-option record and converted WebGL2 name/index lists use the same
captured boundary. Unexpected nested objects are rejected without running
their `toJSON`. Page JSON functions are neither replaced nor frozen.

Scalar validation and encoding use captured numeric intrinsics and indexed
loops, not replaceable `Array.prototype.every`/`map` on private command arrays.
The encoding retains signed zero, NaN and both infinities. Native float-sentinel
replies still receive the existing conversion; ordinary replies avoid a
recursive reviver. String inspection, native-record key inspection and byte
reply branding also use captured functions. A missing `lost` field cannot
consult an inherited author getter or fabricate context loss.

Window and Worker regressions check exact wire text, special-float replies,
primitive-only admission and inherited hooks. A real GPU test poisons page JSON
and inherited `toJSON`, then creates/uploads/queries/deletes a buffer and clears
and reads back actual red pixels without invoking those hooks.

This is a focused boundary fix, not a claim that every WebGL helper is immune
to every prototype mutation. It does not remove author iterators or getters
from Web IDL conversion, bypass GLSL validation, cache native compile status,
increase GPU/command budgets or change context-loss lifecycle policy.
