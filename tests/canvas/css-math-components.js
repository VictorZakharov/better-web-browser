'use strict';
function runCSSMathComponents() {
    const rows = [], failures = [];
    const check = (name, actual, expected, tolerance = 0) => {
        const passed = typeof expected === 'number'
            ? Math.abs(actual - expected) <= tolerance : actual === expected;
        rows.push([typeof actual === 'boolean' ? Number(actual) : actual,
            typeof expected === 'boolean' ? Number(expected) : expected, Number(passed)]);
        if (!passed) failures.push(name + ': ' + actual + ' != ' + expected);
    };
    const canvas = document.createElement('canvas');
    canvas.width = canvas.height = 1;
    const context = canvas.getContext('2d');
    const pixels = color => {
        context.clearRect(0, 0, 1, 1);
        context.fillStyle = color;
        context.fillRect(0, 0, 1, 1);
        return Array.from(context.getImageData(0, 0, 1, 1).data);
    };
    for (const [calculated, literal] of [
        ['rgb(min(100, 255) calc(10 * 2) max(0, 30))', 'rgb(100 20 30)'],
        ['rgb(calc(20%) 30 40)', 'rgb(51 30 40)'],
        ['rgb(20% calc(30) 40)', 'rgb(51 30 40)'],
        ['rgb(calc(100%), min(0%, 20%), max(0%, -1%))', 'red'],
        ['rgb(round(254, 10), sqrt(0), mod(256, 256))', 'rgb(250 0 0)'],
        ['rgb(10 20 30 / min(.25, .5))', 'rgb(10 20 30 / .25)'],
        ['rgb(10, 20, 30, calc(1 / 4))', 'rgb(10 20 30 / .25)'],
        ['rgb(10 20 30/max(10%,25%))', 'rgb(10 20 30 / .25)'],
        ['hsl(calc(1turn / 3) calc(100%) calc(50%))', 'lime'],
        ['hsl(min(120, 240) max(100%, 50%) round(49%, 10%))', 'lime'],
        ['hsl(atan2(1, -1) calc(0%) 50%)', 'rgb(128 128 128)'],
        ['hwb(calc(.5turn) min(20%, 30%) max(10%, 0%))', 'hwb(180 20% 10%)'],
        ['lab(calc(50%) calc(10 + 10) min(30, 40) / calc(.5))', 'lab(50% 20 30 / .5)'],
        ['lch(calc(50) hypot(3, 4) calc(90deg))', 'lch(50 5 90)'],
        ['oklab(calc(.5) calc(.1) calc(-.1))', 'oklab(.5 .1 -.1)'],
        ['oklch(calc(.5) calc(.1) calc(.5turn))', 'oklch(.5 .1 180)'],
        ['color(srgb min(.5, 1) calc(25%) max(0, .75) / calc(.5))', 'color(srgb .5 .25 .75 / .5)'],
        ['r\\67 b(calc(20) min(40, 60) max(40, 60))', 'rgb(20 40 60)']
    ]) {
        check('admission ' + calculated, CSS.supports('color', calculated), true);
        const actual = pixels(calculated), expected = pixels(literal);
        for (let channel = 0; channel < 4; channel++)
            check('paint ' + calculated + ' channel ' + channel, actual[channel], expected[channel], 1);
    }
    for (const invalid of [
        'rgb(calc(20%), 30, 40)', 'rgb(20%, calc(30), 40)',
        'rgb(min(10px, 20px) 20 30)', 'rgb(calc(20% + 1px) 20 30)',
        'rgb(calc(20% + 1) 20 30)', 'rgb(1, 2, none)',
        'rgb(1, 2, 3, none)', 'rgb(1 2 3 / calc(1s))',
        'hsl(calc(120%) 100% 50%)', 'hsl(calc(120ms) 100% 50%)',
        'hsl(0, calc(100), 50%)', 'hsl(none, 100%, 50%)',
        'hwb(0, 10%, 20%)', 'lab(min(20, 30), 0, 0)',
        'color(srgb calc(1px) 0 0)', 'rgb (1 2 3)', 'rgb/**/(1 2 3)'
    ]) {
        check('rejection ' + invalid, CSS.supports('color', invalid), false);
        const element = document.createElement('div');
        element.style.color = 'red';
        const previous = element.style.color;
        element.style.color = invalid;
        check('atomic rejection ' + invalid, element.style.color === previous, true);
    }
    const element = document.getElementById('animated');
    const animation = element.getAnimations()[0];
    check('native CSSAnimation', animation instanceof CSSAnimation, true);
    check('duration', animation.effect.getTiming().duration, 1000);
    check('negative delay', animation.effect.getTiming().delay, -250);
    check('iterations', animation.effect.getTiming().iterations, 2);
    for (const [time, opacity] of [[0, .25], [250, .5], [1000, .75], [1750, 0]]) {
        animation.currentTime = time;
        check('native sample ' + time, Number(getComputedStyle(element).opacity), opacity, .001);
    }
    element.style.animationDuration = 'calc(1s + 1000ms)';
    // No getComputedStyle/getAnimations before this timing query: it must flush
    // pending styles itself, per CSS Animations 2 §6.2.
    check('pending duration flush', animation.effect.getTiming().duration, 2000);
    element.style.animationIterationCount = 'calc(1 + .5)';
    check('pending count flush', animation.effect.getTiming().iterations, 1.5);
    animation.currentTime = 500;
    check('sample after duration update', Number(getComputedStyle(element).opacity), .375, .001);
    element.style.animationDuration = 'calc(1s + 1px)';
    check('invalid duration retained', animation.effect.getTiming().duration, 2000);
    element.style.animationDuration = 'calc(-1s)';
    check('calculated negative duration clamps', animation.effect.getTiming().duration, 0);
    const results = document.getElementById('results');
    for (let start = 0; start < rows.length; start += 8)
        results.setAttribute('data-rows-' + start / 8, JSON.stringify(rows.slice(start, start + 8)));
    results.setAttribute('data-count', String(rows.length));
    results.setAttribute('data-failed', String(failures.length));
    results.setAttribute('data-done', 'true');
    results.textContent = failures.join('\n') || rows.length + ' component/paint/timing checks passed';
    return failures;
}
