// Reporting bridge for the pinned, external Khronos MIT conformance suite.
// No assertion, expected result, shader or API operation in that suite is replaced.
(() => {
    'use strict';
    const requiredExtension = globalThis.__breezeRequiredWebGlExtension;
    const requiredVersion = globalThis.__breezeRequiredWebGlVersion;
    delete globalThis.__breezeRequiredWebGlExtension;
    delete globalThis.__breezeRequiredWebGlVersion;
    // Optional-extension upstream tests may legitimately skip. Require the
    // selected capability independently without wrapping any tested API or
    // retaining a native context that could affect the test's admission budget.
    const requiredAvailable = !requiredExtension || (() => {
        const canvas = document.createElement('canvas');
        const gl = canvas.getContext(requiredVersion === 2 ? 'webgl2' : 'webgl');
        if (!gl) return false;
        const available = Boolean(gl.getExtension(requiredExtension));
        gl.getExtension('WEBGL_lose_context')?.loseContext();
        return available;
    })();
    const results = [];
    let completed = false;
    const wellFormed = value => [...String(value)].map(character => {
        const unit = character.charCodeAt(0);
        return character.length === 1 && unit >= 0xd800 && unit <= 0xdfff ? '\ufffd' : character;
    }).join('');
    const emit = report => {
        const payload = JSON.stringify(report).replace(/[\u007f-\uffff]/g,
            unit => '\\u' + unit.charCodeAt(0).toString(16).padStart(4, '0'));
        const marker = '__BREEZE_WPT_RESULT__';
        if (payload.length <= 8192) { console.log(marker + payload); return; }
        const count = Math.ceil(payload.length / 8192);
        if (count > 128) throw Error('Khronos callback report exceeds transport budget');
        let index = 0;
        const next = () => {
            console.log('__BREEZE_WPT_CHUNK__' + JSON.stringify({index, data:payload.slice(index * 8192, (index + 1) * 8192)}));
            if (++index < count) setTimeout(next, 1);
            else console.log(marker + JSON.stringify({chunks:count}));
        };
        setTimeout(next, 1);
    };
    const append = (success, message, skipped) => results.push({
        name:wellFormed(message), status:skipped ? 'PRECONDITION_FAILED' : success ? 'PASS' : 'FAIL',
        message:success && !skipped ? null : wellFormed(message), stack:null
    });
    window.webglTestHarness = {
        reportResults(path, success, message, skipped = false) {
            if (completed) throw Error('Khronos assertion after completion: ' + path);
            append(Boolean(success), message, Boolean(skipped));
        },
        notifyFinished() {
            if (completed) throw Error('Duplicate Khronos completion');
            completed = true;
            // Khronos treats unsupported optional extensions as valid outcomes.
            // Our selected slice requires real support, so those outcomes must
            // not inflate the pass count or conceal missing implementation.
            if (requiredExtension) {
                append(requiredAvailable, 'Required native capability: ' + requiredExtension, false);
            }
            emit({overall:{status:'OK',message:null},tests:results});
        }
    };
})();
