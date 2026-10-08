'use strict';
function runCSSMathSpacing() {
    const rows=[], failures=[];
    const length = value => value === 'normal' ? 0 : parseFloat(value);
    const check=(name,actual,expected)=>{
        const passed=Number.isFinite(actual) && Math.abs(actual-expected)<=.03;
        rows.push([actual,expected,Number(passed)]);
        if(!passed) failures.push(name+': '+actual+' != '+expected);
    };
    const cases=[
        ['letter-spacing','calc(1em / 5)',f=>f/5],
        ['word-spacing','hypot(.1em,.2em)',f=>f*Math.sqrt(.05)],
        ['letter-spacing','round(.17em,1px)',f=>Math.floor(.17*f+.5)],
        ['word-spacing','calc(-1em / 10)',f=>-f/10],
        ['letter-spacing','mod(.3em,4px)',f=>(.3*f)%4],
        ['word-spacing','min(.5em,12px)',f=>Math.min(.5*f,12)]
    ];
    const probes=document.createElement('main');
    let markup='';
    for(const font of [10,20,30,40]) for(const order of [0,1])
        for(const [property,value] of cases) {
            const size='font-size:'+font+'px;', spacing=property+':'+value+';';
            markup+='<div style="'+(order?size+spacing:spacing+size)+'"></div>';
        }
    probes.innerHTML=markup;
    document.body.appendChild(probes);
    let index=0;
    for(const font of [10,20,30,40]) for(const order of [0,1])
        for(const [property,value,expected] of cases)
            check(property+'/'+font+'/'+order,
                length(getComputedStyle(probes.children[index++]).getPropertyValue(property)),expected(font));
    probes.remove();

    const parent=document.createElement('section');
    parent.style.cssText='font-size:30px;letter-spacing:.2em;word-spacing:.1em';
    parent.innerHTML='<span style="font-size:10px"></span>';
    document.body.appendChild(parent);
    for(const font of [30,40]) {
        parent.style.fontSize=font+'px';
        const style=getComputedStyle(parent.firstElementChild);
        check('inherit/letter/'+font,parseFloat(style.letterSpacing),font/5);
        check('inherit/word/'+font,parseFloat(style.wordSpacing),font/10);
    }
    parent.remove();

    const probe=document.createElement('span');
    document.body.appendChild(probe);
    for(const value of ['normal','0px']) {
        probe.style.letterSpacing=value;
        check('normal/'+value,Number(getComputedStyle(probe).letterSpacing==='normal'),1);
        probe.style.wordSpacing=value;
        check('word-normal/'+value,Number(getComputedStyle(probe).wordSpacing==='0px'),1);
    }

    // Compare width deltas, not font rasterization: five glyphs and two spaces
    // must receive the computed spacing in actual inline-block measurement.
    probe.textContent='a b c';
    for(const font of [20,40]) for(const property of ['letter-spacing','word-spacing']) {
        probe.style.cssText='display:inline-block;white-space:pre;font-size:'+font+'px';
        const baseline=probe.getBoundingClientRect().width;
        for(const sign of [1,-1]) {
            probe.style.setProperty(property,'calc('+sign+'em / 10)');
            check('width/'+property+'/'+font+'/'+sign,
                probe.getBoundingClientRect().width-baseline,
                sign*font/10*(property==='letter-spacing'?5:2));
        }
    }
    for(const value of ['auto','10%','calc(10% + 2px)','calc(1em / 1px)','1s','1deg'])
        for(const property of ['letter-spacing','word-spacing']) {
            probe.style.cssText=property+':2px';
            probe.style.setProperty(property,value);
            check('invalid/'+property+'/'+value,
                parseFloat(getComputedStyle(probe).getPropertyValue(property)),2);
        }
    probe.remove();

    const root=parseFloat(getComputedStyle(document.documentElement).fontSize);
    const relative=document.createElement('div');
    document.body.appendChild(relative);
    for(const property of ['letter-spacing','word-spacing']) {
        relative.style.cssText=property+':calc(1rem - .5em);font-size:10px';
        check('root/'+property,parseFloat(getComputedStyle(relative).getPropertyValue(property)),root-5);
        relative.style.setProperty(property,'calc(1vw - 1em)');
        check('viewport/'+property,parseFloat(getComputedStyle(relative).getPropertyValue(property)),innerWidth/100-10);
    }
    relative.remove();
    const results=document.getElementById('results');
    results.textContent=failures.length?failures.join('\n'):'Passed '+rows.length+' spacing checks';
    results.setAttribute('data-count',String(rows.length));
    results.setAttribute('data-failed',String(failures.length));
    for(let chunk=0;chunk<Math.ceil(rows.length/8);chunk++)
        results.setAttribute('data-rows-'+chunk,JSON.stringify(rows.slice(chunk*8,chunk*8+8)));
    results.setAttribute('data-done','true');
    return failures;
}
