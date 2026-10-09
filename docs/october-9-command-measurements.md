# October 9: measured GPU command transport

These are small, checked workloads, not a whole-browser or game-startup ranking.
The replay protocol and reusable runner are in
[GPU command benchmarks](gpu-command-benchmarks.md). CI remains a smoke gate;
the performance runs and full standards suites run locally.

## Final-source replay: all six samples

Two identical three-repetition series use the final source executable, rotating
before/candidate/Chrome order with fresh profiles. All 36 captures pass actual
state, uniform, final pixel and error checks, with zero reference cleanup
warnings. No builds or full suites from this agent overlap these runs. The
first series has broad timing spread and several regressed medians; the repeat
is retained alongside it, not substituted for it. No sample is discarded.

The before executable already contains the batch's Canvas, resource-checkpoint
and direct-uniform work. The candidate adds the packet/admission/binding slice
and the final private module-settlement repair. These are not merged-main versus
PR-wide timings, nor attribution to one implementation component.

Every setter row performs 100,000 calls after 1,000 warm-up calls, in twenty
bounded tasks. Active elapsed time includes API submission and ordered
`getError` draining, not GPU completion or inter-task timer waits.

| Command, all-six median active ms | Before packets | Final candidate | Chrome 154 |
| --- | ---: | ---: | ---: |
| Constant `clearColor` | 122.40 | 77.95 | 168.55 |
| Constant `uniformMatrix4fv` | 161.95 | 140.60 | 16.65 |
| Constant `uniform4fv` | 150.20 | 106.15 | 14.70 |
| Changing `clearColor` | 112.40 | 62.55 | 7.90 |
| Changing `uniformMatrix4fv` | 148.75 | 110.50 | 15.15 |
| Changing `uniform4fv` | 132.00 | 110.60 | 13.05 |

Aggregate medians are 13–44% lower, but overlapping ranges and the first series'
regressions prevent a robust causal or every-run speedup claim. Uniform medians
remain 7.2–8.5 times Chrome's; changing `clearColor` remains about 7.9 times its
reference cost. Chrome's constant-clear range is particularly unstable and
cannot establish a general Breeze state-change advantage.

| Command, all-six active-ms range | Before packets | Final candidate | Chrome 154 |
| --- | ---: | ---: | ---: |
| Constant `clearColor` | 98.4–271.5 | 55.4–159.6 | 13.1–242.7 |
| Constant `uniformMatrix4fv` | 133.0–401.1 | 92.9–255.4 | 13.1–23.2 |
| Constant `uniform4fv` | 121.2–367.5 | 79.7–223.3 | 10.0–21.7 |
| Changing `clearColor` | 95.2–305.8 | 50.8–197.3 | 5.9–12.0 |
| Changing `uniformMatrix4fv` | 127.5–618.7 | 87.4–290.0 | 12.1–21.6 |
| Changing `uniform4fv` | 117.0–652.4 | 76.1–222.2 | 10.2–19.7 |

Divide these milliseconds by 100 for microseconds per call. The candidate's
median costs are 0.780 / 1.406 / 1.062 / 0.626 / 1.105 / 1.106 microseconds,
in table order. These are elapsed submission/query costs, not isolated CPU cost.

The separate integration control completes 64 WebGL-to-Canvas frames at
1,024 by 512 pixels, with a real `getImageData` barrier per four-frame block and
exact final RGBA checks. It is not isolated `drawImage` or `readPixels` cost.

| Source, all-six median active ms | Before packets | Final candidate | Chrome 154 |
| --- | ---: | ---: | ---: |
| Opaque | 188.30 | 190.90 | 226.10 |
| Premultiplied alpha | 231.50 | 223.65 | 60.80 |
| Straight alpha | 194.05 | 175.00 | 65.30 |

| Source, all-six active-ms range | Before packets | Final candidate | Chrome 154 |
| --- | ---: | ---: | ---: |
| Opaque | 170.4–279.2 | 165.1–290.8 | 201.2–282.6 |
| Premultiplied alpha | 225.4–305.8 | 211.3–271.2 | 48.1–82.0 |
| Straight alpha | 162.3–227.0 | 170.8–197.3 | 53.1–72.0 |

Opaque is 1.4% worse, while premultiplied and straight medians are 3.4% and 9.8%
lower. This is still not a consistent frame-completion gain. Driver/cache state
and other desktop agents are uncontrolled; no scheduling cause is proven.
Further readback/bitmap-copy work needs its own smaller command fixture.

Final-source replay SHA-256 identities:

- Before: `D18757DB506058F3019093339C49CB66A16E77D10C8B20C2DC26B6CC334E56AB`.
- Candidate: `7A5EC5A0EE963E34E2DD2620D6BCF8CA060DAD00FA2F30B6D0C055E83E0253F4`.
- Numeric fixture: `C7547AABECAAFABEEE07F100847F534397074A41AB2132AE8658F678E90236DD`.

Local `head-command-comparison/summary.json` and
`head-command-repeat/summary.json` preserve both series' individual durations,
report paths, work-unit costs and binary/fixture identities. Profiles, captures
and raw reports remain ignored on G:, not in commits. The ordinary unchanged
game capture is separate functionality evidence, not a command speed ranking.

## Earlier transport checkpoint: converted setters

The before executable already includes this batch's Canvas, resource-checkpoint
and direct-uniform work. The after executable adds numeric packets, private JS
admission changes and consecutive native-owner binding reuse together. Thus this
table isolates that combined transport slice, not every change since merged
main, and does not attribute all improvement to one component.

Each row contains 100,000 calls after 1,000 warm-up calls, in twenty bounded
tasks. Active elapsed time includes ordered `getError` queue draining, not GPU
completion or inter-task timer waits. Every phase checks actual uniform/state
values; every run draws and reads the expected green pixel with zero GL errors.

| Command, median active ms | Before packets | After packets | Chrome 154 |
| --- | ---: | ---: | ---: |
| Constant `clearColor` | 95.9 | 54.4 | 106.4 |
| Constant `uniformMatrix4fv` | 128.4 | 88.0 | 11.5 |
| Constant `uniform4fv` | 114.0 | 77.2 | 10.3 |
| Changing `clearColor` | 89.2 | 48.3 | 5.4 |
| Changing `uniformMatrix4fv` | 127.0 | 81.3 | 11.4 |
| Changing `uniform4fv` | 115.8 | 71.8 | 11.0 |

The combined setter slice reduces these medians by 31–46%. Uniform costs remain
roughly 6.5–7.7 times Chrome's, and changing `clearColor` remains 8.9 times its
reference cost. Chrome's much slower constant `clearColor` case must not be
generalized into a claim that Breeze is faster at state changes.

| Command, after cost in microseconds/call | Median | All-sample range |
| --- | ---: | ---: |
| Constant `clearColor` | 0.544 | 0.486–0.550 |
| Constant `uniformMatrix4fv` | 0.880 | 0.831–0.900 |
| Constant `uniform4fv` | 0.772 | 0.742–0.791 |
| Changing `clearColor` | 0.483 | 0.464–0.503 |
| Changing `uniformMatrix4fv` | 0.813 | 0.808–0.884 |
| Changing `uniform4fv` | 0.718 | 0.709–0.744 |

## Earlier checkpoint: completed-frame control

This separate fixture completes 64 WebGL-to-Canvas frames at 1,024 by 512 pixels,
including a `getImageData` barrier for each four-frame block. Final RGBA is
checked for opaque, premultiplied-alpha and straight-alpha sources. It is a
composite integration workload, not an isolated `drawImage` or `readPixels` cost.

| Source, median active ms | Before packets | After packets | Chrome 154 |
| --- | ---: | ---: | ---: |
| Opaque | 152.3 | 170.4 | 197.0 |
| Premultiplied alpha | 200.8 | 199.3 | 46.8 |
| Straight alpha | 163.7 | 175.8 | 52.6 |

This control does **not** demonstrate a frame-completion speedup: opaque and
straight-alpha medians are worse by approximately 12% and 7%; premultiplied
completion is roughly flat. All samples remain in the record. After ranges are
162.0–174.2, 192.9–216.9 and 159.6–189.9 ms respectively. Before ranges are
152.1–163.0, 198.4–223.3 and 158.7–178.1 ms. Overlapping ranges are not proof
that every difference is noise. A readback/bitmap-copy investigation needs its
own smaller command fixture before a causal claim or another optimization.

## Earlier checkpoint: replay identity and limitations

Three repetitions rotate the order of before, after and Chrome. Each browser
has a fresh profile; all eighteen captures run sequentially without this agent's
builds or full suites overlapping. No failed result or timing outlier is omitted.
There are no reference cleanup warnings in this final series.

This workstation also runs other agents. Their scheduling, graphics-driver
caches and thermal state are uncontrolled, so medians and ranges describe these
samples rather than an isolated machine or statistical confidence interval.
Build parallelism is capped at four jobs for this task, and heavy checks are
sequential. Faster individual commands are the optimization target; the
unchanged game remains a separate rendering/cancellation correctness check.

Replay SHA-256 identities:

- Before: `D18757DB506058F3019093339C49CB66A16E77D10C8B20C2DC26B6CC334E56AB`.
- After: `1ADCFAEC3EB9534E0E4F88100760B487A5095E9E3451CFF33D2B2334941668F3`.
- Numeric fixture: `C7547AABECAAFABEEE07F100847F534397074A41AB2132AE8658F678E90236DD`.

These frozen transport checkpoints precede the final private module-settlement
repair. The table does not isolate that repair, and its after hash is not the
final PR executable's identity.

The local `final-command-comparison/summary.json` retains every report path,
fixture/browser hash, individual duration and derived work-unit cost. Generated
reports, profiles and screenshots remain ignored on G:, not in source commits.
