'use strict';
function runCSSMathTransforms() {
    const rows = [], failures = [];
    const check = (name, actual, expected) => {
        const passed = typeof expected === 'number'
            ? Number.isFinite(actual) && Math.abs(actual - expected) <= .02 : actual === expected;
        rows.push([Number(actual), Number(expected), Number(passed)]);
        if (!passed) failures.push(name + ': ' + actual + ' != ' + expected);
    };
    // Only translations: this oracle does not claim support for rotation or scale.
    const cases = [
        ['translate(min(50%, 30px), 0px)', 'translate(max(20%, 50px), 40px)',
            (w,h,f) => [Math.min(w*.5,30),0], (w,h,f) => [Math.max(w*.2,50),40]],
        ['translateX(1em)', 'translateY(25%)',
            (w,h,f) => [f,0], (w,h,f) => [0,h*.25]],
        ['none', 'translate(calc(2em + 10%), max(10px, 25%))',
            (w,h,f) => [0,0], (w,h,f) => [2*f+w*.1,Math.max(10,h*.25)]],
        ['translateX(10px) translateY(20px)', 'translate(calc(20px + 10%), calc(2em))',
            (w,h,f) => [10,20], (w,h,f) => [20+w*.1,2*f]],
        ['matrix(1,0,0,1,10,20)', 'translate(clamp(10px, 20%, 50px), -1em)',
            (w,h,f) => [10,20], (w,h,f) => [Math.max(10,Math.min(w*.2,50)),-f]],
        ['translate(round(23px, 10px), mod(-15px, 10px))', 'translate(hypot(3px, 4px), abs(-30px))',
            (w,h,f) => [20,5], (w,h,f) => [5,30]],
        ['translate(calc(1em + 20%), calc(30% - 10px))', 'none',
            (w,h,f) => [f+w*.2,h*.3-10], (w,h,f) => [0,0]]
    ];
    const sheet = document.createElement('style');
    sheet.textContent = cases.map((c,index) => '@keyframes mathTranslate'+index+
        '{from{transform:'+c[0]+'}to{transform:'+c[1]+'}}').join('\n');
    document.head.appendChild(sheet);
    for (let index=0; index<cases.length; index++) {
        const [from,to,first,last] = cases[index];
        check('from admission '+index, CSS.supports('transform',from), true);
        check('to admission '+index, CSS.supports('transform',to), true);
        for (const css of [false,true]) {
            const element = document.createElement('div');
            element.style.cssText = 'position:absolute;left:20px;top:20px;box-sizing:border-box;margin:0;padding:0;border:0';
            document.body.appendChild(element);
            let animation;
            if (css) {
                element.style.animation = 'mathTranslate'+index+' 1s linear both paused';
                animation = element.getAnimations()[0];
            } else {
                animation = element.animate([{transform:from},{transform:to}], {duration:1000,easing:'linear',fill:'both'});
                animation.pause();
            }
            check('actual animation '+index+' '+css, Boolean(animation), true);
            if (!animation) {element.remove();continue;}
            // Resize and change font without replacing the animation. Percentages
            // must use the target's current border box, not a creation-time size.
            for (const [width,height,font] of [[100,80,20],[400,120,10],[40,40,30]]) {
                element.style.width=width+'px';element.style.height=height+'px';element.style.fontSize=font+'px';
                const a=first(width,height,font),b=last(width,height,font);
                for (const progress of [0,.25,.5,1]) {
                    animation.currentTime=progress*1000;
                    const rect=element.getBoundingClientRect();
                    check('x '+index+' '+css+' '+width+' '+progress, rect.left-20, a[0]+(b[0]-a[0])*progress);
                    check('y '+index+' '+css+' '+width+' '+progress, rect.top-20, a[1]+(b[1]-a[1])*progress);
                }
            }
            animation.cancel();element.remove();
        }
    }
    sheet.remove();
    const results=document.getElementById('results');
    for (let start=0;start<rows.length;start+=8)
        results.setAttribute('data-rows-'+start/8,JSON.stringify(rows.slice(start,start+8)));
    results.setAttribute('data-count',String(rows.length));
    results.setAttribute('data-failed',String(failures.length));
    results.setAttribute('data-done','true');
    results.textContent=failures.join('\n') || rows.length+' native translation checks passed';
    return failures;
}
