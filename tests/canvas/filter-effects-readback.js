// Owned fixture: no live-site code, timings never decide conformance.
try {
const canvas = document.querySelector('#canvas');
const context = canvas.getContext('2d');
const probe = document.querySelector('#probe');
const results = {};
const pixel = (x, y) => Array.from(context.getImageData(x, y, 1, 1).data);
const reset = () => {
    canvas.width = 32;
    context.fillStyle = 'red';
};
const assignments = [
    ' /**/BrIgHtNeSs(calc(2 - 1))/**/ ',
    'brightness(/* default */)',
    'hue-rotate(200grad)',
    'hue-rotate(calc(90deg + 90deg))',
    'blur(calc(1px - 2px))',
    'brightness(calc(2 - 3))',
    'brightness(1)contrast(2)',
    'drop-shadow(currentColor 1em 0)',
    'brightness(2',
    'blur(-1px)',
    'blur(1%)',
    'hue-rotate(calc(0))',
    'brightness(1) bogus()'
];
results.assignments = assignments.map(value => {
    context.filter = 'none';
    context.filter = value;
    return [value, context.filter];
});
reset(); context.filter = 'blur(1px)'; context.fillRect(-2, 3, 2, 4);
results.offCanvasBlur = [pixel(0, 4), pixel(12, 4)];
reset(); context.filter = 'drop-shadow(4px 0 0 blue)'; context.fillRect(-3, 3, 2, 2);
results.offCanvasShadow = [pixel(1, 3), pixel(3, 3)];
reset(); context.filter = 'blur(1px)'; context.fillRect(8, 8, 1, 1);
results.impulse = [pixel(8, 8), pixel(9, 8), pixel(10, 8), pixel(11, 8)];
reset(); context.filter = 'drop-shadow(2px 0 1px blue)'; context.fillRect(8, 8, 1, 1);
results.shadowSigma = [pixel(8, 8), pixel(10, 8), pixel(11, 8), pixel(12, 8)];
reset(); canvas.width = 256; context.font = '100px sans-serif'; context.filter = 'drop-shadow(1em 0 blue)';
canvas.style.fontSize = '9px'; canvas.style.color = 'lime'; context.fillRect(0, 0, 1, 1);
results.assignmentSnapshot = [pixel(2, 0), pixel(9, 0), pixel(100, 0)];
canvas.style.fontSize = '2px'; canvas.style.color = 'blue';
reset(); context.translate(2, 0); context.beginPath(); context.rect(-3, 2, 2, 2);
context.filter = 'drop-shadow(3px 0 0 blue)'; context.fill();
results.currentPath = [pixel(2, 2), context.getTransform().e];
reset(); context.fillRect(2, 2, 2, 2); context.filter = 'drop-shadow(3px 0 0 blue)';
context.drawImage(canvas, 0, 0);
results.selfDraw = [pixel(2, 2), pixel(5, 2)];
results.matrices = [];
for (const filter of ['brightness(.5)', 'contrast(2)', 'saturate(0)',
    'grayscale(1)', 'hue-rotate(200grad)', 'hue-rotate(90deg)', 'sepia(.5)', 'opacity(.5)',
    'brightness(.7)contrast(1.3)sepia(.5)']) {
    reset(); context.filter = filter; context.fillStyle = '#3571b3'; context.fillRect(0, 0, 1, 1);
    results.matrices.push([filter, pixel(0, 0)]);
}
probe.dataset.results = JSON.stringify(results);
for (const [name, value] of Object.entries(results)) {
    if (name === 'assignments' || name === 'matrices') {
        value.forEach((item, index) => probe.setAttribute('data-' + name + index, JSON.stringify(item)));
    } else probe.setAttribute('data-' + name.toLowerCase(), JSON.stringify(value));
}
probe.dataset.done = 'true';
probe.textContent = JSON.stringify(results, null, 2);
} catch (error) {
    document.querySelector('#probe').dataset.error = String(error.stack || error);
    document.querySelector('#probe').textContent = String(error.stack || error);
}
