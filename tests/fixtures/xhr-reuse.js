// The same author-level cancellation sequences run in Breeze and the reference browser.
// Serve the old/new text resources on a loopback fixture origin, not a live site.
async function checkXhrReuse() {
    const results = [];
    const run = mode => new Promise(resolve => {
        const xhr = new XMLHttpRequest();
        const errors = [];
        let replaced = false, afterAbort = null, timer;
        const finish = detail => { clearTimeout(timer); resolve({ mode, ...detail }); };
        const replace = () => {
            if (replaced) return;
            replaced = true;
            xhr.open('GET', 'xhr-reuse-new.txt');
            xhr.send();
        };
        xhr.onabort = () => { errors.push('abort'); if (mode === 'abort-handler') replace(); };
        xhr.onerror = () => errors.push('error');
        xhr.onload = () => finish({ status: xhr.status, text: xhr.responseText.trim(), errors, afterAbort });
        if (mode === 'headers') xhr.onreadystatechange = () => { if (xhr.readyState === 2) replace(); };
        if (mode === 'progress') xhr.onprogress = replace;
        if (mode === 'loadstart-replace') xhr.onloadstart = replace;
        timer = setTimeout(() => finish({ timeout: true, readyState: xhr.readyState, errors }), 1500);
        try {
            xhr.open('GET', 'xhr-reuse-old.txt'); xhr.send();
            if (mode === 'immediate') replace();
            if (mode === 'abort-handler') { xhr.abort(); afterAbort = xhr.readyState; }
        } catch (error) { finish({ exception: error.name, message: error.message }); }
    });
    for (const mode of ['immediate', 'abort-handler', 'headers', 'progress', 'loadstart-replace']) {
        const result = await run(mode);
        result.pass = result.status === 200 && result.text === 'new' &&
            result.errors.join(',') === (mode === 'abort-handler' ? 'abort' : '') &&
            (mode !== 'abort-handler' || result.afterAbort === 1);
        results.push(result);
    }
    try {
        const xhr = new XMLHttpRequest();
        xhr.onloadstart = () => xhr.abort();
        xhr.open('GET', 'xhr-reuse-old.txt'); xhr.send();
        results.push({ mode: 'loadstart-abort', pass: xhr.readyState === 0 && xhr.status === 0 });
    } catch (error) { results.push({ mode: 'loadstart-abort', pass: false, exception: error.name }); }
    return { results, failures: results.filter(result => !result.pass).map(result => result.mode) };
}
