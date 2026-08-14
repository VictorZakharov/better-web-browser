(() => {
    'use strict';

    const marker = '__BREEZE_WPT_RESULT__';
    const harnessStatusName = status => ['OK', 'ERROR', 'TIMEOUT', 'PRECONDITION_FAILED'][status] || `UNKNOWN_${status}`;
    const testStatusName = status => ['PASS', 'FAIL', 'TIMEOUT', 'NOTRUN', 'PRECONDITION_FAILED'][status] || `UNKNOWN_${status}`;
    const optionalString = value => value == null || value === '' ? null : String(value);
    const results = [];

    // Hidden automation consumes callbacks directly and must not make testharness build its
    // interactive DOM report. This is the testharness.js-supported output switch.
    setup({ output: false });

    add_result_callback(test => {
        results.push({
            name: String(test.name),
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
        console.log(marker + JSON.stringify(report));
    });
})();
