{
window.fastRuns++;
const owner = document.currentScript.id;
record('run:' + owner);
document.getElementById('fast').textContent = 'Fast content ready (' + fastRuns + ' elements)';
document.getElementById('fast').style.background = '#b5efce';
const elapsed = Math.round(performance.now() - fixtureStart);
if (fastRuns === 1) document.getElementById('fast').setAttribute('data-first-script-ms', String(elapsed));
console.log('ASYNC_FAST_MS ' + elapsed);
Promise.resolve().then(() => record('micro:' + owner + ':' + (document.currentScript === null)));
}
