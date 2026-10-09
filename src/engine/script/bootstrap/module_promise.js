// Both realms capture the factory before author code. Native V8 Then installs
// these handlers without Promise.prototype.then's author species lookup.
(() => {
    const string = String;
    globalThis.__moduleCompletionHandlers = (operation, completionId) => [
        () => __hostCall(operation, completionId, true, ''),
        reason => {
            let message;
            try { message = string(reason); }
            catch (_) { message = 'Module evaluation rejected with an unprintable reason'; }
            // A diagnostic conversion failure must not leave a Worker starting
            // forever or convert an actual rejected evaluation into fulfillment.
            __hostCall(operation, completionId, false, message);
        }
    ];
})();
