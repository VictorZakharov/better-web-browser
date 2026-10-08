# Bounded author tasks and agent retirement

The execution budget is an implementation resource policy, not a web-standard
deadline or a site-specific exception. Every document and dedicated Worker uses
the same ten-second budget per V8 entry. This replaces the previous two-second
limit, which also interrupted finite startup and compilation work. The private
regular-expression evaluator retains its two-second limit.

The HTML Standard permits user agents to
[abort running scripts](https://html.spec.whatwg.org/multipage/webappapis.html#killing-scripts).
[Worker termination](https://html.spec.whatwg.org/multipage/workers.html#terminate-a-worker)
must also interrupt the current script, not just discard queued tasks. V8's
`IsolateHandle::terminate_execution` is the pinned backend's thread-safe,
uncatchable interruption mechanism. No new dependency or author-exposed switch
is involved.

## Two different outcomes

- A deadline aborts the current entry. After all JavaScript frames unwind, the
  host clears V8's termination bit and can run a later independent task. Partial
  JavaScript/DOM changes remain; abort does not roll them back or retry a timer.
- Explicit `ScriptCancellation::cancel` retires the whole agent. It interrupts
  active execution and rejects all future entries. The owner thread remains
  responsible for closing host resources. A cancellation handle cannot reset
  itself or make a retired document usable again.

Related same-agent document realms share the control. A dedicated Worker has a
separate control created before its thread and entry script start. Its renderer
handle sets both the existing network/mailbox cancellation flag and the engine
control before attempting to enqueue a termination command. A full mailbox is
therefore not a prerequisite for terminating author code. Cancelled workers'
outcomes remain suppressed by the existing renderer event fence.

## Deadline and disposal races

The task generation, retirement state, reason, and backend interruption call
are serialized by a small mutex. Author code never executes under that mutex.
An expired generation cannot terminate a later task, even if the watchdog thread
was descheduled between checking its generation and calling V8. Absolute
monotonic deadlines are sent when tasks arm; delayed queue processing does not
give a task a fresh full budget.

The alarm transport holds one replaceable deadline under a condition variable,
not an unbounded channel of arm messages. Rapid finite-task turnover cannot
accumulate an alarm history if the timer thread is descheduled. Finishing a task
disarms only its own generation; an old disarm cannot erase a replacement.
Spurious wakes always recheck the original absolute deadline. The timer releases
its schedule lock before calling the separate agent cancellation control.

The watchdog disarms and detaches its backend handle before isolate disposal.
Native-binding unwinding also clears the active generation. The thread-local
regular-expression evaluator keeps its non-joining Windows TLS destructor so
loader-lock teardown cannot deadlock.

## This does not relax process containment

The browser still applies its independent renderer Job memory/process limits,
three-second unresponsive notification, and subsequent twelve-second hard-kill
grace. Media frames do not count as document heartbeats. Browser-owned Job
termination bypasses the page command queue; increasing the script budget does
not make browser-owned termination, navigation, tab disposal, or renderer replacement wait ten
seconds. Queued `CancelDocument` alone is not an interrupt of a busy process.

Backend interruption takes effect at V8 execution boundaries. It cannot preempt
arbitrary Rust/OS work inside a native binding. Native operations retain their
own input/work/network bounds, and the browser's process backstop remains the
last-resort containment layer.

## Regression contracts

Local tests cover stale-generation races, idle/pre-start cancellation, terminal
retirement, timeout recovery, real V8 interruption through author `try/catch`,
and Worker entry/message/timer/promise cancellation. Worker tests synchronize
with an author native call rather than sleeping until startup is presumed done.
The renderer-mailbox regression also interrupts a real running Worker while
all 1,024 command slots are occupied. No termination command fits in that queue;
the independent control must stop author code, reject subsequent execution, and
release every queue reservation when the consumer is disposed.
Finite document callbacks and Worker entries lasting more than two seconds must
complete; an infinite timer still aborts under the production deadline and a
later timer still observes its partial state without retrying the aborted task.

This policy change is not a measured speedup. Game-readiness and CPU benchmarks
must report any remaining errors and actual completion rather than treating a
larger timeout as successful gameplay or faster execution.
