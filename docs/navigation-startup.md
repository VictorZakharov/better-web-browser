# Navigation startup measurement

Startup acceptance is visual: navigation acknowledgement, first site content, and a usable
page are separate milestones. A page-ready report is not interchangeable with Chromium's
first-contentful-paint timestamp. Hidden filmstrips use 500 ms sampling, so visual milestone
precision is limited to that interval. These runs do not measure physical keyboard latency.

## Transport negotiation

Breeze enables WinHTTP's HTTP/3 and HTTP/2 protocol flags while retaining HTTP/1.1 fallback.
Unsupported-option/flag errors on older Windows retry HTTP/2, then retain WinHTTP's legacy
default. Other configuration errors remain visible. This does not change certificate validation,
TLS policy, proxy settings, or require QUIC on networks where it is unavailable.

The native transport owns protocol selection. Enabling HTTP/3 is not proof of its use; a fresh
connection can use HTTP/2, and server, network, proxy, and OS capabilities affect negotiation.
See [WinHTTP protocol options](https://learn.microsoft.com/en-us/windows/win32/winhttp/option-flags).

An opt-in live test reports `WINHTTP_OPTION_HTTP_PROTOCOL_USED` on each actual response:

```powershell
$env:BREEZE_PROTOCOL_TEST_URL = 'https://www.youtube.com/'
cargo test --lib reports_actual_negotiated_protocol_without_requiring_http3 -- --ignored --nocapture
```

Flags 0, 1, and 2 mean legacy HTTP, HTTP/2, and HTTP/3 respectively. September 7 local
YouTube probes returned HTTP/2 on all three requests despite allowing HTTP/3. Request headers
arrived in approximately 203, 100, and 112 ms; these are transport probes, not browser load times.

## Live timer scheduling

The live renderer no longer performs six synthetic 250 ms startup timer slices before its first
presentation. Initial microtasks still complete, and document lifecycle events retain their
ordering. Timer callbacks run through subsequent clock-driven event-loop work instead of
advancing 1.5 seconds of timer time during initial script execution. Standalone script-settling
helpers retain their explicitly bounded settling behavior. This is shared by normal and hidden
browser execution, not a benchmark-specific policy or a site exception.

See the [HTML event-loop model](https://html.spec.whatwg.org/multipage/webappapis.html#event-loop-processing-model).

## September 7 exploratory startup samples

Fresh profiles, `https://youtube.com`, 1522-by-692 approximate CSS viewport at 1.25 scale,
hidden/silent runs, 500 ms filmstrips. One sample per configuration is exploratory evidence,
not a stable speed ratio. Network conditions and live responses vary between runs.

| Configuration | Initial presentation / load milestone | Document fetch | Initial script work |
| --- | --- | --- | --- |
| Breeze baseline | Initial presentation 3134 ms | 1275 ms | 1611 ms |
| Breeze modern protocol negotiation | Initial presentation 2896 ms | 813 ms | 1810 ms |
| Breeze modern protocols and real startup timer clock | Initial presentation 2077 ms | 960 ms | 872 ms |
| Chromium reference | First contentful paint 488 ms; load event 1126 ms | Not separately measured | Not separately measured |

The columns are not additive: network/resource processing overlaps and the milestones differ.
The baseline filmstrip was blank at 1.5 seconds and had site content at 3.5 seconds. The last
Breeze run showed a sparse loading shell at 2.5 seconds and the signed-out page message at
5 seconds. Chromium showed loading placeholders by 0.5 seconds and the signed-out page by
1.5 seconds. Earlier Breeze presentation is not equivalent to a usable page, and these results
do not meet the requested Chromium-level startup target. The remaining initial-document
fetch/resource barrier and script work require further profiling and incremental presentation.

The follow-up is now organized as [standards-based loading slices](loading-standards.md),
starting with async script readiness and independently progressing resource batches.
The earlier live-site timings above remain historical observations, not measurements
of those subsequent changes.
