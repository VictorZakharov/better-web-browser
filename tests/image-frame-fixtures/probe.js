'use strict';
// Shared only by this owned probe page, never injected into arbitrary sites.
const frameProbes = [];
const registerFrameProbe = (name, test) => frameProbes.push({name, test});
const frameAssert = (value, message) => { if (!value) throw new Error(message); };
const equalBytes = (actual, expected) => frameAssert(
    Array.from(actual).join(',') === expected.join(','), 'pixel/plane mismatch: '+Array.from(actual));
const rgbaFrame = (options = {}) => new VideoFrame(
    new Uint8Array([10,20,30,255,40,50,60,128]),
    {format:'RGBA',codedWidth:2,codedHeight:1,timestamp:42,duration:30,...options});
let encodedFrameFixtures;
const fixtureBytes = name => {
    const entry = encodedFrameFixtures.find(item => item.name === name);
    if (!entry) throw new Error('Unknown owned image fixture '+name);
    return Uint8Array.from(atob(entry.bytes), character => character.charCodeAt(0));
};
const decodeFixture = async (name, type, options = {}) => {
    const decoder = new ImageDecoder({type, data:fixtureBytes(name), ...options});
    await decoder.tracks.ready;
    return decoder;
};
async function runFrameProbes() {
    let passed = 0;
    try {
        const response = await fetch('frames.json');
        if (!response.ok) throw new Error('Owned fixture data could not be loaded');
        encodedFrameFixtures = await response.json();
        for (const probe of frameProbes) {
            const row = document.createElement('tr');
            row.setAttribute('data-contract', probe.name);
            let status = 'fail', detail = '';
            const start = performance.now();
            try {
                await probe.test();
                status = 'pass'; passed++;
            } catch (error) { detail = error.name+': '+error.message; }
            row.setAttribute('data-status', status);
            row.setAttribute('data-detail', detail);
            row.setAttribute('data-ms', String(performance.now()-start));
            for (const value of [probe.name, status, detail]) {
                const cell = document.createElement('td'); cell.textContent = value; row.appendChild(cell);
            }
            document.querySelector('#results').appendChild(row);
        }
        document.querySelector('#summary').textContent = passed+'/'+frameProbes.length+' contracts passed';
        document.documentElement.setAttribute('data-passed', String(passed));
        document.documentElement.setAttribute('data-total', String(frameProbes.length));
    } catch (error) { document.querySelector('#summary').textContent = error.message; }
    document.documentElement.setAttribute('data-fixture-ready', 'true');
}
