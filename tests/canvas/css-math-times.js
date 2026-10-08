'use strict';
function runCSSMathTimes(includeContextEasing = true) {
    const rows=[], failures=[];
    const check=(name,actual,expected)=>{
        const passed=Number.isFinite(actual) && Math.abs(actual-expected)<=.02;
        rows.push([actual,expected,Number(passed)]);
        if(!passed) failures.push(name+': '+actual+' != '+expected);
    };
    const properties=['animation-duration','animation-delay','transition-duration','transition-delay'];
    const seconds=text=>parseFloat(text)*(text.trim().endsWith('ms')?.001:1);
    const element=document.createElement('section');
    document.body.appendChild(element);
    for(const font of [10,20,30,40]) for(const form of ['longhand','shorthand','negative']) {
        const duration='calc('+(form==='negative'?'-':'')+'1em / 10px * 1s)',
            delay='calc(-1em / 20px * 1s)';
        element.style.cssText='transition-property:opacity;font-size:'+font+'px;'+(form==='shorthand'?
            'animation:none '+duration+' linear '+delay+';transition:opacity '+duration+' linear '+delay:
            'animation-duration:'+duration+';animation-delay:'+delay+';transition-duration:'+duration+';transition-delay:'+delay);
        const style=getComputedStyle(element);
        for(const property of properties) check(form+'/'+font+'/'+property,
            seconds(style.getPropertyValue(property)), property.endsWith('delay')?-font/20:form==='negative'?0:font/10);
    }
    element.style.cssText='transition-property:opacity;font-size:10px;animation-duration:calc(1em / 10px * 1s);'+
        'animation-delay:calc(-1em / 20px * 1s);transition-duration:calc(1em / 10px * 1s);transition-delay:calc(-1em / 20px * 1s)';
    for(const font of [20,40]) {
        element.style.fontSize=font+'px';
        const style=getComputedStyle(element);
        for(const property of properties) check('mutation/'+font+'/'+property,
            seconds(style.getPropertyValue(property)),property.endsWith('delay')?-font/20:font/10);
    }
    const child=document.createElement('div');
    child.style.cssText='font-size:10px;animation:inherit;transition:inherit';
    element.appendChild(child);
    for(const property of properties) check('inherit/'+property,
        seconds(getComputedStyle(child).getPropertyValue(property)),property.endsWith('delay')?-2:4);
    child.remove();
    const rootFont=parseFloat(getComputedStyle(document.documentElement).fontSize);
    element.style.cssText='transition-property:opacity;font-size:10px;animation-duration:calc(1rem / 10px * 1s);'+
        'animation-delay:calc(-1rem / 20px * 1s);transition-duration:calc(1rem / 10px * 1s);transition-delay:calc(-1rem / 20px * 1s)';
    for(const property of properties) check('root/'+property,
        seconds(getComputedStyle(element).getPropertyValue(property)),property.endsWith('delay')?-rootFont/20:rootFont/10);
    element.remove();

    const rule=document.createElement('style');
    rule.textContent='@keyframes context-time-fade{from{opacity:0}to{opacity:1}}';
    document.head.appendChild(rule);
    const animated=document.createElement('div');
    animated.style.cssText='width:20px;height:20px;font-size:20px;animation:context-time-fade '+
        'calc(1em / 10px * 1s) linear calc(-1em / 40px * 1s) both paused';
    document.body.appendChild(animated);
    for(const font of [20,40]) {
        animated.style.fontSize=font+'px';
        const style=getComputedStyle(animated), animation=animated.getAnimations()[0];
        if(!animation) throw Error('missing paused CSS animation');
        const timing=animation.effect.getTiming(), computed=animation.effect.getComputedTiming();
        check('clock/'+font+'/duration',timing.duration,font*100);
        check('clock/'+font+'/delay',timing.delay,-font*25);
        check('clock/'+font+'/progress',computed.progress,.25);
        check('clock/'+font+'/paint',parseFloat(style.opacity),.25);
    }
    if (includeContextEasing) for(const font of [10,20,30,40]) for(const form of ['longhand','shorthand']) {
        const easing='linear(0, calc(1em / 20px))';
        animated.style.cssText='width:20px;height:20px;font-size:'+font+'px;'+
            (form==='shorthand'?'animation:context-time-fade 1s '+easing+' -.5s both paused':
                'animation:context-time-fade 1s linear -.5s both paused;animation-timing-function:'+easing);
        const style=getComputedStyle(animated), animation=animated.getAnimations()[0];
        if(!animation) throw Error('missing context easing animation: '+form+'/'+font);
        const last=value=>parseFloat(value.slice(value.indexOf('(')+1,value.lastIndexOf(')')).split(',').pop());
        check('easing/'+font+'/'+form+'/css',last(style.animationTimingFunction),font/20);
        // CSS animation-timing-function applies between keyframes, not to
        // the effect timing itself. Effect progress remains uneased here.
        check('easing/'+font+'/'+form+'/api',last(animation.effect.getKeyframes()[0].easing),font/20);
        check('easing/'+font+'/'+form+'/progress',animation.effect.getComputedTiming().progress,.5);
        check('easing/'+font+'/'+form+'/paint',parseFloat(style.opacity),font/40);
    }
    for(const font of [10,20,30,40]) for(const form of ['longhand','shorthand'])
        for(const offset of [0,.5]) {
            const count='calc(1em / 10px + '+offset+')';
            animated.style.cssText='font-size:'+font+'px;'+(form==='shorthand'?
                'animation:context-time-fade 1s linear '+count+' both paused':
                'animation:context-time-fade 1s linear both paused;animation-iteration-count:'+count);
            const animation=animated.getAnimations()[0];
            if(!animation) throw Error('missing context iteration animation');
            animation.currentTime=10000;
            const style=getComputedStyle(animated), expected=font/10+offset;
            check('iterations/'+font+'/'+form+'/'+offset+'/css',parseFloat(style.animationIterationCount),expected);
            check('iterations/'+font+'/'+form+'/'+offset+'/api',animation.effect.getTiming().iterations,expected);
            check('iterations/'+font+'/'+form+'/'+offset+'/end',animation.effect.getComputedTiming().endTime,expected*1000);
            check('iterations/'+font+'/'+form+'/'+offset+'/paint',parseFloat(style.opacity),offset===0?1:.5);
        }
    animated.remove();
    rule.remove();
    const results=document.getElementById('results');
    results.textContent=failures.length?failures.join('\n'):'Passed '+rows.length+' time checks';
    results.setAttribute('data-count',String(rows.length));
    results.setAttribute('data-failed',String(failures.length));
    for(let chunk=0;chunk<Math.ceil(rows.length/8);chunk++)
        results.setAttribute('data-rows-'+chunk,JSON.stringify(rows.slice(chunk*8,chunk*8+8)));
    results.setAttribute('data-done','true');
    return failures;
}
