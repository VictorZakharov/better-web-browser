'use strict';
function runCSSAnimationOverrides() {
    const rows=[],failures=[];
    const check=(name,actual,expected) => {
        const passed=typeof expected==='number'
            ? Number.isFinite(actual) && Math.abs(actual-expected)<=.002 : actual===expected;
        rows.push([typeof actual==='number' ? actual : Number(passed),
            typeof expected==='number' ? expected : 1,Number(passed)]);
        if (!passed) failures.push(name+': '+actual+' != '+expected);
    };
    let next=0;
    const fresh=(state='paused') => {
        const element=document.createElement('div'),sheet=document.createElement('style'),name='ownership'+next++;
        sheet.textContent='@keyframes '+name+'{from{opacity:0}to{opacity:1}}';
        document.head.appendChild(sheet);document.body.appendChild(element);
        element.style.animation=name+' 1s linear both '+state;
        const animation=element.getAnimations()[0];
        check('real CSSAnimation '+name,animation instanceof CSSAnimation,true);
        const replace=upper => {
            sheet.sheet.deleteRule(0);
            sheet.sheet.insertRule('@keyframes '+name+'{from{opacity:0}to{opacity:'+upper+'}}',0);
        };
        return {element,sheet,animation,replace,close:()=>{animation.cancel();element.remove();sheet.remove();}};
    };
    {
        const c=fresh(),effect=c.animation.effect;
        effect.updateTiming({duration:1000});
        c.element.style.animationDuration='calc(2s + 2s)';c.element.style.animationDelay='min(2s, 1s)';
        const timing=effect.getTiming();
        check('API duration wins',timing.duration,1000);check('CSS delay still applies',timing.delay,1000);
        c.animation.currentTime=1250;
        check('mixed ownership paints native opacity',Number(getComputedStyle(c.element).opacity),.25);
        check('author CSS remains unchanged',getComputedStyle(c.element).animationDuration,'4s');
        c.close();
    }
    {
        const c=fresh(),effect=c.animation.effect;
        effect.updateTiming({iterations:2,direction:'reverse'});
        c.element.style.animationIterationCount='sqrt(16)';c.element.style.animationDirection='alternate';
        c.element.style.animationDelay='200ms';c.element.style.animationFillMode='forwards';
        const timing=effect.getTiming();
        check('API count',timing.iterations,2);check('API direction',timing.direction,'reverse');
        check('CSS unclaimed delay',timing.delay,200);check('CSS unclaimed fill',timing.fill,'forwards');
        c.animation.currentTime=450;check('reverse sample',Number(getComputedStyle(c.element).opacity),.75);
        c.close();
    }
    {
        const c=fresh(),effect=c.animation.effect;
        effect.updateTiming({delay:-250,fill:'both'});
        c.element.style.animationDelay='500ms';c.element.style.animationFillMode='none';
        c.element.style.animationIterationCount='4';c.element.style.animationDirection='alternate';
        const timing=effect.getTiming();
        check('API delay',timing.delay,-250);check('API fill',timing.fill,'both');
        check('CSS unclaimed count',timing.iterations,4);check('CSS unclaimed direction',timing.direction,'alternate');
        c.animation.currentTime=0;
        check('negative delay timing progress',effect.getComputedTiming().progress,.25);
        check('negative delay sample',Number(getComputedStyle(c.element).opacity),.25);
        c.close();
    }
    {
        const c=fresh(),effect=c.animation.effect;
        effect.updateTiming({easing:'steps(4)',endDelay:250,iterationStart:.5});
        c.element.style.animationDuration='4s';c.element.style.animationTimingFunction='ease-in';
        const timing=effect.getTiming();
        check('unmapped easing survives',timing.easing,'steps(4)');check('unmapped end delay survives',timing.endDelay,250);
        check('unmapped iteration start survives',timing.iterationStart,.5);check('CSS duration still applies',timing.duration,4000);
        c.close();
    }
    for (const input of [{},{duration:undefined},{unknown:2000}]) {
        const c=fresh();c.animation.effect.updateTiming(input);c.element.style.animationDuration='2s';
        check('absent duration does not claim ownership',c.animation.effect.getTiming().duration,2000);c.close();
    }
    {
        const c=fresh(),effect=c.animation.effect;let reads=0;
        const inherited=Object.create({get duration(){reads++;return 1500;}});
        effect.updateTiming(inherited);c.element.style.animationDuration='3s';
        check('inherited member is a dictionary value',effect.getTiming().duration,1500);
        check('dictionary getter read once',reads,1);c.close();
    }
    for (const input of [{duration:2000,iterations:-1},{duration:2000,easing:'linear(.5)'},42]) {
        const c=fresh(),effect=c.animation.effect;let threw=false;
        try {effect.updateTiming(input);} catch(error) {threw=error instanceof TypeError;}
        check('invalid timing throws',threw,true);check('failed timing is atomic',effect.getTiming().duration,1000);
        c.element.style.animationDuration='3s';
        check('failed timing does not claim duration',effect.getTiming().duration,3000);c.close();
    }
    {
        const c=fresh(),effect=c.animation.effect;
        effect.setKeyframes([{opacity:.2},{opacity:.6}]);c.replace(.9);
        c.element.style.animationTimingFunction='ease-in';c.element.style.animationDuration='2s';
        const frames=effect.getKeyframes();
        check('API first frame',Number(frames[0].opacity),.2);check('API last frame',Number(frames[1].opacity),.6);
        check('API frame easing survives CSS',frames[0].easing,'linear');check('CSS timing not claimed by frames',effect.getTiming().duration,2000);
        c.animation.currentTime=1000;check('API frames still paint',Number(getComputedStyle(c.element).opacity),.4);
        c.sheet.sheet.deleteRule(0);c.element.getAnimations();check('removing final rule still cancels',c.animation.playState,'idle');c.close();
    }
    {
        const c=fresh(),effect=c.animation.effect;let threw=false;
        try {effect.setKeyframes([{opacity:.2,offset:-1},{opacity:.6}]);} catch(error) {threw=error instanceof TypeError;}
        check('invalid frames throw',threw,true);c.replace(.8);
        check('failed frame update does not claim frames',Number(effect.getKeyframes()[1].opacity),.8);
        c.animation.currentTime=500;check('CSS frames still paint',Number(getComputedStyle(c.element).opacity),.4);c.close();
    }
    {
        const c=fresh(),original=c.animation.effect;
        const replacement=new KeyframeEffect(c.element,[{opacity:.3},{opacity:.7}],{duration:500,fill:'both'});
        c.animation.effect=replacement;c.replace(.9);c.element.style.animationDuration='4s';
        check('replacement duration unaffected by CSS',replacement.getTiming().duration,500);
        check('replacement frames unaffected by CSS',Number(replacement.getKeyframes()[1].opacity),.7);
        c.animation.currentTime=250;check('replacement paints',Number(getComputedStyle(c.element).opacity),.5);
        c.animation.effect=null;c.element.style.animationDelay='1s';document.getAnimations();
        check('null effect survives CSS update',c.animation.effect,null);
        c.animation.effect=original;c.element.style.animationDuration='8s';
        check('reattaching original does not restore CSS ownership',original.getTiming().duration,1000);c.close();
    }
    for (const operation of ['pause','play']) {
        const c=fresh();c.animation[operation]();
        c.element.style.animationPlayState='running';c.element.getAnimations();
        c.element.style.animationPlayState='paused';c.element.getAnimations();
        check('successful '+operation+' claims play state',c.animation.playState,operation==='pause'?'paused':'running');c.close();
    }
    for (const operation of ['reverse','startTime']) {
        const c=fresh();c.animation.currentTime=500;
        if(operation==='reverse')c.animation.reverse();else c.animation.startTime=0;
        c.element.style.animationPlayState='running';c.element.getAnimations();
        c.element.style.animationPlayState='paused';c.element.getAnimations();
        check(operation+' changed paused state and claims ownership',c.animation.playState,'running');c.close();
    }
    for (const operation of ['reverse','startTime']) {
        const c=fresh('running');c.animation.currentTime=500;
        if(operation==='reverse')c.animation.reverse();else c.animation.startTime=0;
        c.element.style.animationPlayState='paused';c.element.getAnimations();
        check(operation+' without paused-state change leaves CSS ownership',c.animation.playState,'paused');c.close();
    }
    // The policy belongs to the base API algorithm, not subclass dispatch.
    for (const operation of ['play','pause','reverse','startTime']) {
        const c=fresh();c.animation.currentTime=500;
        if(operation==='startTime')Object.getOwnPropertyDescriptor(Animation.prototype,'startTime').set.call(c.animation,0);
        else Animation.prototype[operation].call(c.animation);
        c.element.style.animationPlayState='running';c.element.getAnimations();
        c.element.style.animationPlayState='paused';c.element.getAnimations();
        check('base '+operation+' owns CSS state',c.animation.playState,operation==='pause'?'paused':'running');c.close();
    }
    {
        const c=fresh(),replacement=new KeyframeEffect(c.element,[{opacity:.2},{opacity:.8}],{duration:500,fill:'both'});
        Object.getOwnPropertyDescriptor(Animation.prototype,'effect').set.call(c.animation,replacement);
        c.element.style.animationDuration='4s';
        check('base setter owns effect replacement',replacement.getTiming().duration,500);c.close();
    }
    const results=document.getElementById('results');
    for(let start=0;start<rows.length;start+=8)results.setAttribute('data-rows-'+start/8,JSON.stringify(rows.slice(start,start+8)));
    results.setAttribute('data-count',String(rows.length));results.setAttribute('data-failed',String(failures.length));
    results.setAttribute('data-done','true');results.textContent=failures.join('\n')||rows.length+' CSS/API ownership checks passed';
    return failures;
}
