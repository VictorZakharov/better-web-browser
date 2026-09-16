(() => {
    'use strict';

    const marker = '__BREEZE_WPT_RESULT__';
    const harnessStatusName = status => ['OK', 'ERROR', 'TIMEOUT', 'PRECONDITION_FAILED'][status] || `UNKNOWN_${status}`;
    const testStatusName = status => ['PASS', 'FAIL', 'TIMEOUT', 'NOTRUN', 'PRECONDITION_FAILED'][status] || `UNKNOWN_${status}`;
    // Rust JSON strings contain Unicode scalar values, while JavaScript strings may
    // contain isolated UTF-16 surrogates. Match TextEncoder's replacement behavior
    // at this diagnostics-only boundary so arbitrary WPT names remain reportable.
    const wellFormed = value => [...String(value)].map(character => {
        const unit = character.charCodeAt(0);
        return character.length === 1 && unit >= 0xD800 && unit <= 0xDFFF ? '\uFFFD' : character;
    }).join('');
    const optionalString = value => value == null || value === '' ? null : wellFormed(value);
    const results = [];

    // Hidden automation consumes callbacks directly and must not make testharness build its
    // interactive DOM report. This is the testharness.js-supported output switch.
    setup({ output: false });

    add_result_callback(test => {
        results.push({
            name: wellFormed(test.name),
            status: testStatusName(test.status),
            message: optionalString(test.message),
            stack: optionalString(test.stack)
        });
    });

    add_completion_callback((_tests, status) => {
        const report = {
            overall: {
                status: harnessStatusName(status.status),
                message: optionalString(status.message)
            },
            tests: results
        };
        // ASCII JSON also prevents chunk boundaries from splitting surrogate pairs.
        const payload = JSON.stringify(report).replace(/[\u007f-\uffff]/g,
            unit => '\\u' + unit.charCodeAt(0).toString(16).padStart(4, '0'));
        // Keep each diagnostic within the normal IPC text limit. Large upstream
        // files still report every subtest; the reader rejects missing chunks.
        if (payload.length <= 8192) {
            console.log(marker + payload);
        } else {
            const count = Math.ceil(payload.length / 8192);
            if (count > 128) throw new Error('WPT callback report exceeds transport budget');
            let index = 0;
            const emitChunk = () => {
                console.log('__BREEZE_WPT_CHUNK__' + JSON.stringify({ index, data: payload.slice(index * 8192, (index + 1) * 8192) }));
                index++;
                if (index < count) setTimeout(emitChunk, 1);
                else console.log(marker + JSON.stringify({ chunks: count }));
            };
            // A runtime wakeup can batch ready tasks. A positive delay leaves the
            // next chunk pending for a later wakeup and its bounded report.
            setTimeout(emitChunk, 1);
        }
    });
})();
