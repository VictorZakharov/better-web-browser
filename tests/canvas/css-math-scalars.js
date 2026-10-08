'use strict';
function runCSSMathScalars() {
    const rows=[], failures=[];
    const check=(name,actual,expected)=>{
        const passed=Number.isFinite(actual) && Math.abs(actual-expected)<=.02;
        rows.push([actual,expected,Number(passed)]);
        if(!passed) failures.push(name+': '+actual+' != '+expected);
    };
    const cases=[
        ['opacity','calc((sign(1em - 20px) + 1) / 2)',f=>f<20?0:f===20?.5:1],
        ['flex-grow','hypot(calc(1em / 10px),0)',f=>f/10],
        ['flex-shrink','round(calc(1em / 7px))',f=>Math.floor(f/7+.5)],
        ['z-index','calc(-1em / 20px)',f=>Math.floor(-f/20+.5)],
        ['line-height','calc(1em / 10px)',f=>f*f/10],
        ['aspect-ratio','calc(1em / 10px) / 2',f=>f/20],
        ['opacity','calc(1em / 100px)',f=>f/100],
        ['z-index','calc(16777217 + 1em / 40px)',f=>Math.floor(16777217+f/40+.5)]
    ];
    const probes=document.createElement('main');
    let markup='';
    for(const font of [10,20,30,40]) for(const order of [0,1]) {
        for(const [property,value] of cases) {
            const size='font-size:'+font+'px;', scalar=property+':'+value+';';
            markup+='<div style="'+(order?size+scalar:scalar+size)+'"></div>';
        }
    }
    probes.innerHTML=markup;
    document.body.appendChild(probes);
    let index=0;
    for(const font of [10,20,30,40]) for(const order of [0,1]) {
        for(const [property,value,expected] of cases) {
            const text=getComputedStyle(probes.children[index++]).getPropertyValue(property);
            const ratio=text.split('/');
            const actual=property==='aspect-ratio'?parseFloat(ratio[0])/parseFloat(ratio[1]):parseFloat(text);
            check(property+'/'+font+'/'+order,actual,expected(font));
        }
    }
    probes.remove();

    const parent=document.createElement('section');
    parent.style.cssText='font-size:30px;opacity:calc(1em / 100px);flex-grow:calc(1em / 10px);'+
        'flex-shrink:calc(1em / 20px);z-index:calc(1em / 1px);line-height:calc(1em / 10px)';
    parent.innerHTML='<span style="font-size:10px;opacity:inherit;flex:inherit;z-index:inherit"></span>';
    document.body.appendChild(parent);
    for(const font of [30,40]) {
        parent.style.fontSize=font+'px';
        const style=getComputedStyle(parent.firstElementChild);
        for(const [property,expected] of [['opacity',font/100],['flex-grow',font/10],
            ['flex-shrink',font/20],['z-index',font],['line-height',font]])
            check('inherit/'+font+'/'+property,parseFloat(style.getPropertyValue(property)),expected);
    }
    parent.remove();

    const flex=document.createElement('section');
    flex.style.cssText='display:flex;width:200px;font-size:30px';
    flex.innerHTML='<div style="flex:calc(1em / 10px) 1 0px;min-width:0"></div>'+
        '<div style="flex:1 1 0px;min-width:0"></div>';
    document.body.appendChild(flex);
    for(const [font,expected] of [[30,150],[10,100]]) {
        flex.style.fontSize=font+'px';
        check('grow/'+font,flex.children[0].getBoundingClientRect().width,expected);
        check('sibling/'+font,flex.children[1].getBoundingClientRect().width,200-expected);
    }
    flex.remove();

    const rootSize=parseFloat(getComputedStyle(document.documentElement).fontSize);
    const root=document.createElement('div');
    root.style.cssText='font-size:10px;opacity:calc(1rem / 100px);line-height:calc(1rem / 10px)';
    document.body.appendChild(root);
    check('root/opacity',parseFloat(getComputedStyle(root).opacity),rootSize/100);
    check('root/line-height',parseFloat(getComputedStyle(root).lineHeight),rootSize);
    root.remove();
    const results=document.getElementById('results');
    results.textContent=failures.length?failures.join('\n'):'Passed '+rows.length+' scalar checks';
    results.setAttribute('data-count',String(rows.length));
    results.setAttribute('data-failed',String(failures.length));
    for(let chunk=0;chunk<Math.ceil(rows.length/8);chunk++)
        results.setAttribute('data-rows-'+chunk,JSON.stringify(rows.slice(chunk*8,chunk*8+8)));
    results.setAttribute('data-done','true');
    return failures;
}
