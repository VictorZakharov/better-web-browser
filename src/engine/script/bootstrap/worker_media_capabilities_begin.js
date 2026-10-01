(() => {
    'use strict';
    const host = (...args) => __hostCall(...args);
    const windowObject = globalThis;
    // Capture the scheduler before author code can replace global timers, then
    // remove the bootstrap hook. The worker exposes queries, not capture APIs.
    const queueMediaTask = globalThis.__mediaCapabilitiesQueue;
    delete globalThis.__mediaCapabilitiesQueue;
