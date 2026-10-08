'use strict';
function runCSSMathEasing() {
    const rows = [], failures = [];
    const check = (name, actual, expected) => {
        const passed = typeof expected === 'number'
            ? Math.abs(actual - expected) <= .002 : actual === expected;
        rows.push([Number(actual), Number(expected), Number(passed)]);
        if (!passed) failures.push(name + ': ' + actual + ' != ' + expected);
    };
    const cases = [
        ['cubic-bezier(calc(1 / 4), min(0, 1), max(.5, .75), sqrt(1))', 'cubic-bezier(.25, 0, .75, 1)'],
        ['cubic-bezier(min(.5, 0), calc(-.5), max(.5, 1), calc(1.5))', 'cubic-bezier(0, -.5, 1, 1.5)'],
        ['steps(calc(2.5), jump-none)', 'steps(3, jump-none)'],
        ['steps(round(4.1), start)', 'steps(4, start)'],
        ['steps(sqrt(16))', 'steps(4, end)'],
        ['steps(calc(-1))', 'steps(1, end)'],
        ['st\\65ps(calc(4), j\\75mp-start)', 'steps(4, jump-start)'],
        ['e\\61se', 'ease'],
        ['steps(calc(2 + 2), jump-both)', 'steps(4, jump-both)'],
        ['linear(calc(0), sqrt(.25) min(20%, 30%) max(70%, 50%), calc(1))', 'linear(0, .5 20% 70%, 1)'],
        ['linear(calc(0%) calc(20%) min(0, 1), max(80%, 60%) calc(100%) sqrt(1))', 'linear(0 0% 20%, 1 80% 100%)'],
        ['linear(calc(-1) -10%, calc(2) 110%)', 'linear(-1 -10%, 2 110%)'],
        ['linear(calc(0) 75%, sqrt(.25) 25%, calc(1))', 'linear(0 75%, .5 25%, 1)']
    ];
    const samplePair = (calculated, literal, css) => {
        const elements = [document.createElement('div'), document.createElement('div')];
        const animations = [];
        for (let index = 0; index < 2; index++) {
            const easing = index ? literal : calculated;
            document.body.appendChild(elements[index]);
            if (css) {
                elements[index].style.animation = 'mathEase 1s ' + easing + ' both paused';
                animations.push(elements[index].getAnimations()[0]);
            } else {
                const animation = elements[index].animate([{opacity:0}, {opacity:1}],
                    {duration:1000, fill:'both', easing});
                animation.pause();
                animations.push(animation);
            }
        }
        check('actual animations ' + calculated, animations.every(Boolean), true);
        for (const time of [0, 125, 250, 375, 500, 750, 1000]) {
            for (const animation of animations) animation.currentTime = time;
            check((css ? 'CSS' : 'WA') + ' sample ' + time + ' ' + calculated,
                Number(getComputedStyle(elements[0]).opacity), Number(getComputedStyle(elements[1]).opacity));
        }
        for (const animation of animations) animation.cancel();
        for (const element of elements) element.remove();
    };
    for (const [calculated, literal] of cases) {
        check('stylesheet admission ' + calculated, CSS.supports('animation-timing-function', calculated), true);
        samplePair(calculated, literal, true);
        samplePair(calculated, literal, false);
    }
    for (const invalid of [
        'linear(.4)', 'cubic-bezier(0x0, 0, 1, 1)', 'steps (4)',
        'cubic-bezier(calc(-1), 0, 1, 1)', 'cubic-bezier(calc(1px), 0, 1, 1)',
        'cubic-bezier(25%, 0, 1, 1)', 'steps(calc(1.4), jump-none)',
        'steps(calc(1s))', 'linear(0, calc(1px))', 'linear(calc(0%) 0 calc(20%), 1)',
        'linear(0 calc(10% + 1px), 1)', 'linear(calc(0) calc(1), 1)'
    ]) {
        check('type rejection ' + invalid, CSS.supports('animation-timing-function', invalid), false);
        const element = document.createElement('div');
        let threw = false;
        try {element.animate([{opacity:0}, {opacity:1}], {duration:1000, easing:invalid});}
        catch (error) {threw = error instanceof TypeError;}
        check('Web Animations rejection ' + invalid, threw, true);
    }
    const results = document.getElementById('results');
    for (let start = 0; start < rows.length; start += 8)
        results.setAttribute('data-rows-' + start / 8, JSON.stringify(rows.slice(start, start + 8)));
    results.setAttribute('data-count', String(rows.length));
    results.setAttribute('data-failed', String(failures.length));
    results.setAttribute('data-done', 'true');
    results.textContent = failures.join('\n') || rows.length + ' easing admission/progress checks passed';
    return failures;
}
