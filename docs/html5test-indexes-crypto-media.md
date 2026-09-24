# IndexedDB indexes, Canvas text, Web Crypto, WebVTT, and Gamepad

This is a cross-API standards batch, not HTML5test-specific behavior. The
implementation follows the [Indexed Database API](https://w3c.github.io/IndexedDB/),
[HTML Canvas](https://html.spec.whatwg.org/multipage/canvas.html),
[Web Cryptography](https://www.w3.org/TR/webcrypto/),
[HTML text tracks](https://html.spec.whatwg.org/multipage/media.html#timed-text-tracks),
[WebVTT](https://www.w3.org/TR/webvtt1/), and
[Gamepad](https://www.w3.org/TR/gamepad/) specifications. The technical-alpha
security and compatibility limitations in the README still apply.

## Compatibility contracts

- IndexedDB index definitions live with their object-store schema and persist
  across restarts. The browser derives index keys from stored structured clones;
  script cannot supply an alternate projection. Unique and multi-entry
  constraints are checked in the same atomic transaction as writes and
  versionchange backfill. Index get/getKey/getAll/getAllKeys/count and
  next/prev/unique cursors use index-key then primary-key ordering.
- Canvas `font`, `textAlign`, `textBaseline`, `direction`, `measureText`,
  `fillText`, and `strokeText` use the page renderer's font shaping and glyph
  rasterization path. The renderer bounds text, glyph count, and raster bytes.
  Text is not represented by synthetic rectangles or sent to the browser
  process as remote font data.
- Secure Window and dedicated-worker realms expose native CNG-backed
  SHA-1/256/384/512 digests, HMAC, PBKDF2, HKDF, AES-CBC/CTR/GCM/KW,
  ECDH/ECDSA on P-256/P-384/P-521, RSA-OAEP, RSA-PSS, and
  RSASSA-PKCS1-v1_5. The binding checks key type, algorithm, extractability,
  and usages. AES-KW follows RFC 3394 and rejects modified ciphertext before
  returning unwrapped material. RSA supports JWK, SPKI, and PKCS#8 with a
  strict bounded DER parser.
- Text tracks expose live cue lists, cue activation and events, track load/error
  behavior, and WebVTT timing/settings parsing. At most four current caption
  cues are painted over the corresponding retained video frame and clipped to
  its rectangle; page DOM is not modified to display captions.
- `navigator.getGamepads()` polls up to four XInput slots. A physical device is
  hidden from pages until button, trigger, or stick interaction; exposed
  gamepads use the standard mapping and dispatch connection events.

## Known boundaries

IndexedDB worker exposure and complete cross-tab scheduling are pending.
Canvas text metrics and complex-script typography have targeted tests, not a
complete Canvas conformance claim. Web Crypto does not offer Ed25519/X25519,
RSA exponents other than 65537 for generated keys, hardware keys, or every
Key Format/algorithm combination. Synchronous PBKDF2 derivation is limited to
1,000,000 iterations per call to bound renderer CPU work. WebVTT regions,
vertical cue painting, and full cue styling are absent. Gamepad currently
covers XInput controllers only,
without haptics. These gaps are not hidden by feature-probe substitutes.

## Verification and score

The RFC 3394 wrap vector, digest/HMAC/PBKDF2/HKDF/AES vectors, RSA and EC
roundtrips/tamper cases, IndexedDB atomicity and ordering, Canvas pixels,
WebVTT parsing/painting, and Window/Worker API contracts are local regression
tests. The complete `cargo test --locked` Windows suite passed on this branch.

The 2026-09-24 fresh-profile hidden release build rendered **396 / 588** on
HTML5test.co at 1280×720, 125% scale, `en-US`, after 10 seconds' settle,
with no JavaScript errors or renderer exits. The preceding released standards
slice measured **379 / 588** under the same display/locale conditions: **+17**
capability points. This is not a performance comparison or a claim of complete
conformance. No HTML5test-specific paths, probe results, or fixtures are used
to implement the APIs.
