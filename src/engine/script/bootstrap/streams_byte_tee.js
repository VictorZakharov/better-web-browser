(() => {
    'use strict';

    // A byte tee forwards BYOB demand to its source. Reading through a
    // default reader would hide byobRequest and can stall minimum-fill reads.
    globalThis.__byteTee = source => {
        if (source.locked) throw new TypeError('ReadableStream is locked');
        const branches = [{}, {}];
        let reading = false, readAgain = false, currentReader = null, resolveCancel;
        const cancellation = new Promise(resolve => { resolveCancel = resolve; });
        const fail = error => {
            for (const branch of branches)
                if (!branch.cancelled) branch.controller.error(error);
            resolveCancel();
        };
        const pull = () => {
            if (reading) { readAgain = true; return; }
            reading = true;
            const pending = branches.find(branch => !branch.cancelled &&
                branch.controller.__stream.__byobReads.length);
            let reader;
            try {
                reader = source.getReader(pending ? { mode: 'byob' } : undefined);
            } catch (error) { reading = false; fail(error); return; }
            currentReader = reader;
            const request = pending && pending.controller.__stream.__byobReads[0];
            const read = request ? reader.read(new Uint8Array(
                request.buffer, request.offset + request.filled,
                request.capacity - request.filled)) : reader.read();
            read.then(({ value, done }) => {
                currentReader = null;
                reader.releaseLock();
                reading = false;
                // The source BYOB read transfers the branch buffer. The
                // returned view owns its replacement and earlier bytes.
                if (request && value) request.buffer = value.buffer;
                if (done) {
                    for (const branch of branches) {
                        if (branch.cancelled) continue;
                        branch.controller.close();
                        branch.controller.byobRequest?.respond(0);
                    }
                    resolveCancel();
                    return;
                }
                // Prepare independent copies before enqueue transfers input.
                const chunks = [pending === branches[1]
                    ? new Uint8Array(value) : value, new Uint8Array(value)];
                for (let index = 0; index < 2; index++) {
                    const branch = branches[index];
                    if (branch.cancelled) continue;
                    if (branch === pending &&
                        branch.controller.__stream.__byobReads.includes(request)) {
                        branch.controller.byobRequest.respond(value.byteLength);
                    } else branch.controller.enqueue(chunks[index]);
                }
                if (readAgain) {
                    readAgain = false;
                    const needsData = branches.some(branch => !branch.cancelled &&
                        (branch.controller.__stream.__byobReads.length ||
                            branch.controller.__stream.__reads.length ||
                            branch.controller.desiredSize > 0));
                    if (needsData) pull();
                }
            }, error => {
                currentReader = null;
                reader.releaseLock();
                reading = false;
                fail(error);
            });
        };
        const streams = branches.map(branch => new ReadableStream({
            type: 'bytes',
            start(controller) { branch.controller = controller; },
            pull,
            cancel(reason) {
                branch.cancelled = true;
                branch.reason = reason;
                if (branches.every(item => item.cancelled)) {
                    const cancel = currentReader ? currentReader.cancel(
                        branches.map(item => item.reason)) : source.cancel(
                        branches.map(item => item.reason));
                    resolveCancel(cancel);
                }
                return cancellation;
            }
        }));
        (source.__errorObservers ??= []).push(fail);
        if (source.__state === 'errored') fail(source.__storedError);
        return streams;
    };
})();
