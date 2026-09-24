    // CSP3 §5.1: synthetic events are constructible, while enforcement-created
    // events are queued by the policy engine after the blocked operation.
    // https://www.w3.org/TR/CSP3/#violation-events
    class SecurityPolicyViolationEvent extends Event {
        constructor(type, init = {}) {
            if (arguments.length === 0) throw new TypeError('Event type is required');
            super(type, init);
            init = init == null ? {} : Object(init);
            for (const name of ['documentURI', 'referrer', 'blockedURI',
                'effectiveDirective', 'violatedDirective', 'originalPolicy',
                'sourceFile', 'sample']) {
                Object.defineProperty(this, name, {
                    value: init[name] === undefined ? '' : String(init[name]), enumerable: true
                });
            }
            const disposition = init.disposition === undefined ? 'enforce' : String(init.disposition);
            if (disposition !== 'enforce' && disposition !== 'report')
                throw new TypeError('Invalid CSP disposition');
            Object.defineProperty(this, 'disposition', { value: disposition, enumerable: true });
            for (const [name, bits] of [['statusCode', 16], ['lineNumber', 32], ['columnNumber', 32]]) {
                const value = init[name] === undefined ? 0 : Number(init[name]);
                Object.defineProperty(this, name, {
                    value: bits === 16 ? value & 0xffff : value >>> 0, enumerable: true
                });
            }
        }
    }
    globalThis.SecurityPolicyViolationEvent = SecurityPolicyViolationEvent;
