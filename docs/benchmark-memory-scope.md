# Comparing browser memory without conflating process roles

The Chromium reference harness's existing `private_bytes`, `working_set_bytes`,
`cpu_time_ms` and `process_count` describe its whole owned process tree. They are
not the Chromium renderer's memory. Breeze's `renderer_private_bytes` describes
the isolated renderer, including its in-process ANGLE/D3D resources and native
compiler work. A direct comparison of those two private-byte fields is invalid.

The Chromium harness now labels its aggregate with
`memory_scope: owned_browser_process_tree` and supplies `process_memory.roles`.
Windows memory/CPU samples and Chromium's browser-target
`SystemInfo.getProcessInfo` role labels produce separate browser, renderer, GPU,
utility, other and unattributed buckets. Multiple renderer processes are summed
within the renderer bucket; this is not necessarily one document's memory.

The OS ancestry/creation-time check establishes process ownership independently.
CDP identities only label processes already in that snapshot: a role reply does
not authorize opening an arbitrary PID. The output contains finite role names
and aggregate sizes/counts, not command lines, URLs or PID lists. Unknown role
text becomes `other`; missing labels become `unattributed`.

The role query has a five-second upper bound and uses the owned loopback browser
target, not the author page's JavaScript. Its cost occurs after page/rendering
measurements. Unsupported, malformed, oversized or duplicate-identity replies
retain the OS aggregate and report an attribution error. Missing evidence is
never converted into zero renderer memory or silently dropped. Successful role
rows partition exactly the same OS sample as the aggregate.

`sum_of_process_peak_working_set_bytes` deliberately identifies its semantics:
it sums each process's lifetime peak, which may occur at different times. It is
not a simultaneous browser-tree high-water measurement. Neither private commit
nor working set measures GPU VRAM. A resource-accounting ledger is another
quantity again, and must not be substituted for either OS measurement.

Breeze additionally retains fixed-size `renderer_memory_observations` in the
broker, independent of an author task that may be busy or hung. A failed query
or renderer exit marks the current sample unavailable but preserves the last
successful observation and high-water fields. Private commit's high-water is
explicitly **observed** at the existing one-second broker cadence; it can miss
short spikes and is not an exact native peak. The working-set high-water retains
the OS peak counter. Exited sessions remain in this evidence array even though
the legacy current-live-process totals exclude them. Never interpret a crash's
zero current renderer total as a successfully measured zero-memory page.

## Controlled renderer-budget experiments

An isolated memory-limit experiment must freeze both executables, document the
hard Windows Job limit and use identical GPU ledger ceilings, page revision,
viewport, scale factor, cache/profile mode and observation endpoints. Soft
pressure and emergency-headroom policies derived from that Job budget must be
reported as such. A successful first paint or lobby DOM is insufficient: inspect
the actual 3D output and allocation/compiler errors, and allow a settled period
to observe whether memory plateaus or continues growing.

Larger experimental budgets are diagnostic evidence, not permission to ship
relaxed containment. The separately authorized opt-in 3D policy is a typed,
persisted user choice, not an experimental global constant; its 2 GiB Job limit
must be reported alongside its GPU ledger ceilings. Restore any temporary
experimental constants before final shipping verification.
Report incomplete rendering, cleanup errors and mismatched sampling scopes as
limitations rather than interpreting them as performance victories.

References: [CDP process information](https://chromedevtools.github.io/devtools-protocol/tot/SystemInfo/#method-getProcessInfo),
[Windows private usage and working-set counters](https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-process_memory_counters_ex),
and [Windows recycled parent-process IDs](https://devblogs.microsoft.com/oldnewthing/20200122-00/?p=103355).
