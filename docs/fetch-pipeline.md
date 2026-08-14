# Fetch and navigation pipeline

Breeze routes top-level navigations and document subresources through one typed request/response
boundary. The model exists independently of JavaScript APIs so future `fetch`, XHR, modules, and
workers do not each invent URL, origin, redirect, cookie, or CORS behavior.

The compatibility references are the [WHATWG Fetch Standard](https://fetch.spec.whatwg.org/), the
[WHATWG URL Standard](https://url.spec.whatwg.org/), and Microsoft's
[WinHTTP documentation](https://learn.microsoft.com/windows/win32/winhttp/about-winhttp). URL
parsing and resolution use the standards-oriented [`url` crate](https://docs.rs/url/); Breeze does
not maintain another general-purpose URL parser.

## Ownership boundaries

The platform-neutral `fetch` module owns:

- HTTP(S) URLs and tuple origins
- ordered header lists and the script request-header guard
- request context, destination, mode, credentials, redirect, referrer, and abort state
- typed response filtering and the distinction between HTTP responses and Fetch failures
- bounded bodies that can be consumed incrementally through `std::io::Read`
- CORS safelists, response checks, exposed-header filtering, and preflight validation

The Windows transport owns only platform work:

- proxy discovery, DNS, connection reuse, HTTP framing, and content decompression
- TLS negotiation and certificate-chain verification
- one HTTP exchange at a time, with automatic WinHTTP redirects and cookies disabled

Automatic WinHTTP authentication is also disabled until Breeze has an explicit HTTP-auth
credential store; ambient operating-system credentials must not bypass `CredentialsMode`.

TLS and certificate verification remain entirely inside WinHTTP. Project code must not disable,
replace, or reproduce those checks.

## Request policies

| Context | Default mode | Credentials | Redirects | Response visibility |
|---|---|---|---|---|
| Top-level navigation | `navigate` | include | follow | internal response |
| Classic subresource | `no-cors` | same-origin | follow | internal response |
| Font subresource | `cors` | same-origin | follow | internal response after CORS check |
| Script-initiated Fetch | `cors` | same-origin | follow | basic/CORS/opaque filtered response |

The transport processes redirects explicitly, stores eligible response cookies before following,
removes credentials when policy or origin changes require it, and applies a bounded redirect count.
HTTP errors such as 404 or 503 are responses with bodies; DNS, connection, abort, redirect-policy,
body-budget, and CORS failures are typed `FetchError` values.

## Cancellation and resource bounds

Every active document owns a `FetchController`. Navigation aborts the previous controller before
creating the next one, so blocking, deferred, and asynchronous resources share one abort signal.
Workers check that signal before and after WinHTTP operations and between response-body chunks.

WinHTTP is currently used synchronously on background workers. Microsoft warns against closing a
synchronous request handle from another thread while an operation is pending, so cancellation does
not use that unsafe shortcut. A platform call already in progress is allowed to return, after which
the request handle is dropped and no further body or redirect work occurs. Moving the transport to
asynchronous WinHTTP would allow an OS-level wakeup without changing the Fetch-facing contract.

Each response has an explicit byte ceiling (16 MiB by default), enforced while chunks arrive. The
document layer retains its separate aggregate page-resource budget.

## Offline verification

Networking integration tests use only ephemeral loopback servers. The matrix covers redirect
modes, cookies across redirect and path scopes, shared cancellation, HTTP versus network failures,
CORS success/failure, preflight success/failure, guarded script headers, referrer reduction, and
response-body limits. No public service is part of the conformance contract.
