// Owned, deterministic privacy/lifetime fixture. No cross-origin network traffic.
(async () => {
    const probe = document.querySelector('#probe');
    const results = {};
    const canvas = () => {
        const surface = document.createElement('canvas');
        surface.width = surface.height = 4;
        return surface;
    };
    const raises = (callback, name) => {
        try { callback(); return false; }
        catch (error) { return error.name === name; }
    };
    const taint = surface => {
        const context = surface.getContext('2d');
        context.filter = 'drop-shadow(1px 0 currentColor)';
        context.fillRect(0, 0, 1, 1);
        return context;
    };
    try {
        for (const filter of ['drop-shadow(1px 0 currentColor)', 'drop-shadow(1px 0)']) {
            const surface = canvas(), context = surface.getContext('2d');
            context.save(); context.filter = filter;
            const assignmentIsClean = !raises(() => surface.toDataURL(), 'SecurityError');
            context.fillRect(0, 0, 1, 1); context.restore();
            const readIsDenied = raises(() => context.getImageData(0, 0, 1, 1), 'SecurityError');
            const exportIsDenied = raises(() => surface.toDataURL(), 'SecurityError');
            context.clearRect(0, 0, 4, 4);
            context.putImageData(new ImageData(4, 4), 0, 0);
            const clearDoesNotClean = raises(() => surface.toDataURL(), 'SecurityError');
            surface.width = 4;
            const resizeCleans = !raises(() => surface.toDataURL(), 'SecurityError');
            results[filter.includes('currentColor') ? 'explicitColor' : 'implicitColor'] =
                [assignmentIsClean, readIsDenied, exportIsDenied, clearDoesNotClean, resizeCleans];
        }
        const source = canvas(); taint(source);
        const target = canvas(), drawing = target.getContext('2d');
        drawing.drawImage(source, 0, 0);
        results.drawing = [raises(() => target.toDataURL(), 'SecurityError')];
        target.width = 4;
        const pattern = drawing.createPattern(source, 'repeat');
        const creationIsClean = !raises(() => target.toDataURL(), 'SecurityError');
        source.width = 4;
        drawing.fillStyle = pattern;
        const fillAssignmentTaints = raises(() => target.toDataURL(), 'SecurityError');
        target.width = 4; drawing.strokeStyle = pattern;
        results.pattern = [creationIsClean, fillAssignmentTaints,
            raises(() => target.toDataURL(), 'SecurityError')];

        taint(source);
        const bitmap = await createImageBitmap(source, 0, 0, 2, 2,
            {resizeWidth: 4, resizeHeight: 4});
        const copy = await createImageBitmap(bitmap);
        results.clone = [];
        for (const image of [bitmap, copy]) {
            results.clone.push(raises(() => structuredClone(image), 'DataCloneError'),
                raises(() => structuredClone(image, {transfer: [image]}), 'DataCloneError'),
                image.width === 4);
        }
        const output = canvas(), renderer = output.getContext('bitmaprenderer');
        renderer.transferFromImageBitmap(bitmap);
        const transferTaints = raises(() => output.toDataURL(), 'SecurityError');
        renderer.transferFromImageBitmap(null);
        const resetCleans = !raises(() => output.toDataURL(), 'SecurityError');
        renderer.transferFromImageBitmap(copy);
        const secondTransferTaints = raises(() => output.toDataURL(), 'SecurityError');
        renderer.transferFromImageBitmap(await createImageBitmap(new ImageData(2, 2)));
        results.renderer = [transferTaints, resetCleans, secondTransferTaints,
            !raises(() => output.toDataURL(), 'SecurityError'), bitmap.width === 0, copy.width === 0];

        const offscreen = new OffscreenCanvas(4, 4), offscreenContext = taint(offscreen);
        let blobDenied = false;
        try { await offscreen.convertToBlob(); }
        catch (error) { blobDenied = error.name === 'SecurityError'; }
        const transferred = offscreen.transferToImageBitmap();
        target.width = 4; drawing.drawImage(transferred, 0, 0);
        results.offscreen = [blobDenied, offscreenContext.getImageData(0, 0, 1, 1).data[3] === 0,
            raises(() => target.toDataURL(), 'SecurityError')];
        for (const [name, values] of Object.entries(results))
            probe.setAttribute('data-' + name.toLowerCase(), JSON.stringify(values));
        probe.dataset.pass = String(Object.values(results).flat().every(Boolean));
        probe.dataset.done = 'true';
        probe.textContent = JSON.stringify(results, null, 2);
    } catch (error) {
        probe.dataset.error = String(error.stack || error);
        probe.textContent = probe.dataset.error;
    }
})();
