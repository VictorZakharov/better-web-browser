// Original, deterministic contracts shared by Breeze and headless Chromium.
const audioProbes = [];
const audioProbe = (name, callback) => audioProbes.push({name, callback});
const requireAudio = (condition, message) => {
    if (!condition) throw new Error(message);
};
const sameSamples = (actual, expected, tolerance = 0) => {
    requireAudio(actual.length === expected.length, 'sample count');
    for (let index = 0; index < expected.length; index++) {
        requireAudio(Math.abs(actual[index] - expected[index]) <= tolerance,
            `sample ${index}: ${actual[index]} versus ${expected[index]}`);
    }
};
const audioFailure = (operation, name) => {
    let failure;
    try { operation(); } catch (error) { failure = error; }
    requireAudio(failure?.name === name, `expected ${name}, received ${failure?.name}`);
};
const ownedAudio = (frames = 960, channels = 1, rate = 48000, timestamp = 0) => {
    const samples = new Float32Array(frames * channels);
    for (let frame = 0; frame < frames; frame++) {
        for (let channel = 0; channel < channels; channel++) {
            samples[frame * channels + channel] =
                Math.sin(frame * (330 + channel * 110) * Math.PI * 2 / rate) * .25;
        }
    }
    return new AudioData({format:'f32', sampleRate:rate, numberOfChannels:channels,
        numberOfFrames:frames, timestamp, data:samples});
};
async function runAudioProbes() {
    let passed = 0;
    const failures = [];
    // A bounded subset helps isolate a reference-browser failure without
    // changing the inputs or silently weakening the full acceptance run.
    const parameters = new URLSearchParams(location.search);
    const begin = Number(parameters.get('begin') ?? 0);
    const end = Number(parameters.get('end') ?? audioProbes.length);
    const selected = audioProbes.slice(begin,end);
    for (const probe of selected) {
        const row = document.createElement('tr');
        const name = document.createElement('td');
        const status = document.createElement('td');
        const detail = document.createElement('td');
        name.textContent = probe.name;
        try {
            const result = await probe.callback();
            row.dataset.status = 'pass';
            status.textContent = 'PASS';
            detail.textContent = result ?? '';
            passed++;
        } catch (error) {
            row.dataset.status = 'fail';
            status.textContent = 'FAIL';
            detail.textContent = error.name + ': ' + error.message;
            failures.push(probe.name);
        }
        row.append(name, status, detail);
        row.dataset.contract = probe.name;
        row.dataset.detail = detail.textContent;
        document.getElementById('results').append(row);
        console.log('AUDIO ' + probe.name + ': ' + status.textContent);
    }
    const summary = document.getElementById('summary');
    summary.dataset.passed = String(passed);
    summary.dataset.total = String(selected.length);
    summary.dataset.failures = failures.join(',');
    summary.textContent = `${passed}/${selected.length} audio contracts passed`;
    document.documentElement.dataset.fixtureReady = 'true';
}
