(() => {
    'use strict';
    const isDocument = typeof document !== 'undefined';
    async function measure(realm) {
        const token = realm + '-' + Math.random().toString(36).slice(2);
        const start = performance.now();
        let timerFired = false;
        setTimeout(() => { timerFired = true; }, 50);
        const response = await fetch('/chunks?token=' + token);
        const headerMs = Math.round(performance.now() - start);
        const phaseAtHeadersPromise = fetch('/phase?token=' + token).then(response => response.json());
        const cloneText = response.clone().text();
        const reader = response.body.getReader();
        const first = await reader.read();
        const firstMs = Math.round(performance.now() - start);
        const timerBeforeEnd = timerFired;
        const phaseAtFirstPromise = fetch('/phase?token=' + token).then(response => response.json());
        let body = new TextDecoder().decode(first.value);
        for (;;) {
            const chunk = await reader.read();
            if (chunk.done) break;
            body += new TextDecoder().decode(chunk.value);
        }
        const cloned = await cloneText;
        const completionMs = Math.round(performance.now() - start);
        const [phaseAtHeaders, phaseAtFirst] = await Promise.all([phaseAtHeadersPromise, phaseAtFirstPromise]);
        return { realm, headerMs, firstMs, completionMs,
            headersBeforeEnd: phaseAtHeaders < 2, firstBeforeEnd: phaseAtFirst < 2,
            timerBeforeEnd, bodyCorrect: body === 'f'.repeat(65536) + 'last' && cloned === body };
    }
    if (!isDocument) {
        measure('worker').then(result => postMessage({ result }), error => postMessage({ error: String(error) }));
        return;
    }
    const worker = new Worker('/progressive-fetch.js');
    const workerResult = new Promise((resolve, reject) => {
        worker.onmessage = event => event.data.error ? reject(new Error(event.data.error)) : resolve(event.data.result);
        worker.onerror = event => reject(new Error(event.message));
    });
    Promise.all([measure('document'), workerResult]).then(results => {
        let passed = 0;
        for (const result of results)
            for (const key of ['headersBeforeEnd', 'firstBeforeEnd', 'timerBeforeEnd', 'bodyCorrect'])
                if (result[key]) passed++;
        document.querySelector('#result').textContent = (passed === 8 ? 'PASS ' : 'FAIL ') +
            passed + '/8\n' + JSON.stringify(results, null, 2);
        console.log('progressive_fetch_result', JSON.stringify({ passed, results }));
    }, error => { document.querySelector('#result').textContent = 'FAIL ' + error; })
        .finally(() => { worker.terminate(); document.documentElement.dataset.fixtureReady = 'true'; });
})();
